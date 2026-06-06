# Security Policy

These crates implement authentication and cryptographic primitives, so we take
vulnerability reports seriously.

## Reporting a vulnerability

**Please do not open a public issue for security vulnerabilities.**

Instead, report privately via one of:

- GitHub's [private vulnerability reporting](https://docs.github.com/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability)
  on this repository (**Security → Report a vulnerability**), or
- email **klauthed@klauthed.com**.

Please include:

- the affected crate(s) and version(s),
- a description of the issue and its impact,
- a minimal reproduction if possible.

We aim to acknowledge reports within a few business days and will keep you
updated on remediation and disclosure timing. Please give us a reasonable window
to release a fix before any public disclosure.

## Scope

Issues in the cryptographic and authentication code (`klauthed-security`,
`klauthed-protocol`, and the auth/token surface of `klauthed-web`) are the
highest priority. Note that these libraries are pre-1.0 and under active
development; APIs and security properties may change between releases.
