# Security Policy

## Scope

This project includes:

- A local desktop application built with Tauri
- Local terminal execution and repository access
- Optional local HTTP access
- Optional mobile pairing flows

Security reports are especially valuable for issues involving:

- Command execution boundaries
- Local HTTP authentication and authorization
- Mobile pairing flows
- Secret handling
- Workspace and filesystem isolation

## Reporting a vulnerability

Do not open public GitHub issues for suspected vulnerabilities.

Report security issues privately to the project maintainer with:

- A short description of the issue
- Steps to reproduce
- Impact assessment
- Any proof-of-concept or logs needed to validate the report

If the issue involves credentials, tokens, certificates, or private repositories, redact them before sharing.

## Response expectations

The project will try to:

- Acknowledge the report
- Reproduce and validate the issue
- Prepare a fix or mitigation
- Coordinate public disclosure after a fix is available

## Operational notes

- The local HTTP and pairing features should be treated as privileged surfaces.
- The default local-only configuration is safer than exposing the backend to a wider network.
- If remote access is enabled, use explicit authentication and encrypted transport.
