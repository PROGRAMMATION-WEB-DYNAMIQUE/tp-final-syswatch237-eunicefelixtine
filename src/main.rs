use chrono::Local;
use std::fmt;
use sysinfo::{System, Process};
use std::thread;
use std::time::Duration;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::fs::OpenOptions;

// --- Types métier ----------------------------------------------------------

#[derive(Debug, Clone)]
struct CpuInfo {
    usage_percent: f32,
    core_count: usize,
}

#[derive(Debug, Clone)]
struct MemInfo {
    total_mb: u64,
    used_mb: u64,
    free_mb: u64,
}

#[derive(Debug, Clone)]
struct ProcessInfo {
    pid: u32,
    name: String,
    cpu_usage: f32,
    memory_mb: u64,
}

#[derive(Debug, Clone)]
struct SystemSnapshot {
    timestamp: String,
    cpu: CpuInfo,
    memory: MemInfo,
    top_processes: Vec<ProcessInfo>,
}

// --- Affichage (trait Display) --------------------------------------------

impl fmt::Display for CpuInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CPU: {:.1}% ({} cœurs)", self.usage_percent, self.core_count)
    }
}

impl fmt::Display for MemInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "MEM: {}MB utilisés / {}MB total ({} MB libres)",
            self.used_mb, self.total_mb, self.free_mb
        )
    }
}

impl fmt::Display for ProcessInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "  [{:>6}] {:<25} CPU:{:>5.1}%  MEM:{:>5}MB",
            self.pid, self.name, self.cpu_usage, self.memory_mb
        )
    }
}

impl fmt::Display for SystemSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "=== SysWatch — {} ===", self.timestamp)?;
        writeln!(f, "{}", self.cpu)?;
        writeln!(f, "{}", self.memory)?;
        writeln!(f, "--- Top Processus ---")?;
        for p in &self.top_processes {
            writeln!(f, "{}", p)?;
        }
        write!(f, "=====================")
    }
}

// --- Collecte réelle des métriques ---------------------------------------

fn collect_snapshot() -> Result<SystemSnapshot, String> {
    let mut sys = System::new_all();

    sys.refresh_all();                     // mémoire + processus
    thread::sleep(Duration::from_millis(500));
    sys.refresh_cpu_all();                 // pour sysinfo 0.32+

    let cpu_usage = sys.global_cpu_usage();
    let core_count = sys.physical_core_count().unwrap_or(1);

    let total_mb = sys.total_memory() / 1_048_576;
    let used_mb  = sys.used_memory()  / 1_048_576;
    let free_mb  = sys.free_memory()  / 1_048_576;

    let mut processes: Vec<&Process> = sys.processes().values().collect();
    processes.sort_by(|a, b| b.cpu_usage().partial_cmp(&a.cpu_usage()).unwrap());

    let top_processes: Vec<ProcessInfo> = processes
        .iter()
        .take(5)
        .map(|p| {
            let name = p.name().to_string_lossy().to_string();
            ProcessInfo {
                pid: p.pid().as_u32(),
                name,
                cpu_usage: p.cpu_usage(),
                memory_mb: p.memory() / 1_048_576,
            }
        })
        .collect();

    Ok(SystemSnapshot {
        timestamp: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        cpu: CpuInfo { usage_percent: cpu_usage, core_count },
        memory: MemInfo { total_mb, used_mb, free_mb },
        top_processes,
    })
}

// --- Formatage des réponses selon la commande ----------------------------

fn format_response(snapshot: &SystemSnapshot, command: &str) -> String {
    match command.trim().to_lowercase().as_str() {
        "cpu" => format!("{}", snapshot.cpu),
        "mem" => format!("{}", snapshot.memory),
        "ps"  => {
            let mut out = "Top processus :\n".to_string();
            for p in &snapshot.top_processes {
                out.push_str(&format!("{}\n", p));
            }
            out
        }
        "all" => format!("{}", snapshot),
        "help" => r#"Commandes disponibles :
  cpu  - affiche l'utilisation CPU
  mem  - affiche la mémoire
  ps   - liste les 5 processus les plus actifs
  all  - affiche tout (comme l'affichage par défaut)
  quit - ferme la connexion
  help - affiche cette aide"#.to_string(),
        "quit" => "quit".to_string(),
        _ => format!("Commande inconnue : {}\nTape 'help' pour la liste.", command),
    }
}

// --- Journalisation -------------------------------------------------------

fn log_message(msg: &str) {
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
    let log_line = format!("[{}] {}\n", timestamp, msg);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("syswatch.log")
        .unwrap_or_else(|_| {
            eprintln!("Impossible d'ouvrir le fichier de log, création d'un nouveau fichier.");
            std::fs::File::create("syswatch.log").unwrap()
        });
    let _ = file.write_all(log_line.as_bytes());
}

// --- Gestion d'un client (dans son propre thread) ------------------------

fn handle_client(mut stream: TcpStream, snapshot_arc: Arc<Mutex<SystemSnapshot>>) {
    let addr = stream.peer_addr().unwrap();
    log_message(&format!("Connexion de {}", addr));
    println!("Client connecté : {}", addr);

    let reader = BufReader::new(stream.try_clone().unwrap());
    for line in reader.lines() {
        match line {
            Ok(cmd) => {
                log_message(&format!("Commande de {} : {}", addr, cmd));
                let current_snapshot = snapshot_arc.lock().unwrap().clone();
                let response = format_response(&current_snapshot, &cmd);
                if response.trim() == "quit" {
                    let _ = writeln!(stream, "Au revoir !");
                    break;
                }
                let _ = writeln!(stream, "{}", response);
            }
            Err(_) => break,
        }
    }
    log_message(&format!("Déconnexion de {}", addr));
    println!("Client déconnecté : {}", addr);
}

// --- MAIN : serveur TCP multi‑threadé avec rafraîchissement périodique ---

fn main() {
    // Snapshot initial (vide, sera écrasé rapidement)
    let snapshot_arc = Arc::new(Mutex::new(SystemSnapshot {
        timestamp: String::new(),
        cpu: CpuInfo { usage_percent: 0.0, core_count: 0 },
        memory: MemInfo { total_mb: 0, used_mb: 0, free_mb: 0 },
        top_processes: vec![],
    }));

    // Thread dédié au rafraîchissement toutes les 5 secondes
    let snapshot_updater = Arc::clone(&snapshot_arc);
    thread::spawn(move || loop {
        if let Ok(new_snapshot) = collect_snapshot() {
            let mut guard = snapshot_updater.lock().unwrap();
            *guard = new_snapshot;
        }
        thread::sleep(Duration::from_secs(5));
    });

    // Lancement du serveur TCP sur le port 7878
    let listener = TcpListener::bind("127.0.0.1:7878").expect("Impossible de bind le port 7878");
    println!("Serveur SysWatch démarré sur 127.0.0.1:7878");
    log_message("Serveur démarré");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let snapshot_clone = Arc::clone(&snapshot_arc);
                thread::spawn(move || handle_client(stream, snapshot_clone));
            }
            Err(e) => eprintln!("Connexion échouée : {}", e),
        }
    }
}