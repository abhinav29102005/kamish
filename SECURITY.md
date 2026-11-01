# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.0-alpha.x | :white_check_mark: |

## Reporting a Vulnerability

The Kamish development team takes security and cryptographic integrity seriously. If you discover a vulnerability, residency enforcement bypass, consensus split-brain hazard, or secrets leakage in Kamish, please report it privately.

**DO NOT file a public GitHub issue.**

### Reporting Procedure
1. Email details of the vulnerability to `security@kamish.local` (or designated project maintainer).
2. Include:
   - Reproduction steps or proof-of-concept exploit script.
   - Affected components (`cmd/kamish`, `kamish-broker`, `kamish-gateway`, `packages/kamish-client`, etc.).
   - Impact assessment (confidentiality, data sovereignty residency violation, consensus failure).
3. We will acknowledge receipt within 48 hours and provide a coordinated disclosure timeline (typically 90 days, or faster for critical cryptographic bypasses).

## Security Architecture & Best Practices
- **Authentication**: Zero default credentials ship with Kamish. Run `kamish init` to generate a secure administrator account or pass credentials via file.
- **Residency Enforcement**: All cross-region replication is validated against cryptographic Ed25519-signed policy documents with monotonically increasing version floors.
- **Audit Trails**: Every administrative action, partition lease, and sovereignty verdict is written to an append-only SHA-256 hash-chained log.
