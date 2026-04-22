# SysWatch – Moniteur système en réseau

## Description
SysWatch est un serveur TCP écrit en Rust qui permet de surveiller et contrôler à distance une machine Windows. Il collecte en temps réel les métriques système (CPU, mémoire, processus) et expose un shell textuel sur le port `7878`.

## Fonctionnalités
- Authentification par token (`ENSPD2026`)
- Commandes disponibles :
  - `cpu`  – utilisation CPU
  - `mem`  – mémoire RAM
  - `ps`   – top 5 processus
  - `all`  – affichage complet
  - `help` – aide
  - `quit` – déconnexion
- Rafraîchissement automatique des données toutes les 5 secondes
- Multi‑threading : gestion simultanée de plusieurs clients
- Journalisation de toutes les connexions et commandes dans `syswatch.log`

## Utilisation
1. Lancer le serveur : `cargo run --release`
2. Se connecter depuis un autre poste (ou la même machine) : `telnet <IP_serveur> 7878`
3. Saisir le token `ENSPD2026`, puis les commandes.

## Prérequis
- Rust (éditions 2021)
- Dépendances : `sysinfo = "0.32"`, `chrono = "0.4"`
- Pare‑feu : autoriser le port 7878 en entrée (pour l’accès distant)

## Auteur
Projet réalisé dans le cadre du cours de programmation système.