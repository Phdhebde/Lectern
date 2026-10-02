# Security policy

## Reporting a vulnerability

Please do **not** open a public issue. Report vulnerabilities privately through the
repository's *Security › Report a vulnerability* (GitHub private vulnerability reporting),
with a description, the affected version and reproduction steps.

We acknowledge reports within 3 business days, keep you informed, and publish a fix and an
advisory once it is available. We credit reporters who wish to be credited.

## Supported versions

Security fixes go to the latest minor release. Instances should follow releases closely;
dependency updates are monitored in CI (RustSec and npm audits).

## Scope

The platform code in this repository. Instance configuration, hosting and content are the
responsibility of each operator. See [docs/security.md](docs/security.md) for the
security design.
