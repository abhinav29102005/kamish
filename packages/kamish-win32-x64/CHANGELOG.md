# Changelog

## 0.1.0-alpha.1 (2026-11-01)

### Features
- Unified all-in-one distribution model embedding broker, gateway, and dashboard into a single platform-native binary (ADR-006).
- Implemented `kamish start`: runs embedded broker, gateway, and dashboard in one process.
- Implemented `kamish doctor`: checks port availability, etcd quorum connectivity, and configuration integrity.
- Implemented `kamish init`: first-run configuration and administrator account provisioning.
- Implemented `kamish status`: cluster and sovereignty consensus model inspection.
- Implemented `kamish topic create|list`: sovereign topic registration.
- Implemented `kamish policy sign|verify|publish`: Ed25519 residency policy management.
- Implemented shell completions for bash, zsh, fish, and PowerShell.
- Integrated optionalDependencies platform binary distribution for offline, verifiable installs.
