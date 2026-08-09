# Security Policy

## Supported versions

Until the first stable release, only the latest tagged `0.x` release receives security fixes.

## Reporting a vulnerability

Please do not open a public issue for vulnerabilities that could expose configuration content, bypass path or ownership checks, or write outside the documented safety boundary.

Use GitHub's private vulnerability reporting feature for this repository. Include the affected version, operating environment, reproduction steps and expected impact. You should receive an acknowledgement within seven days.

Config Editor intentionally does not elevate privileges, manage `/etc`, install packages, execute services or connect to remote machines. Reports that require those capabilities are outside the current threat model unless the documented boundary can still be bypassed.
