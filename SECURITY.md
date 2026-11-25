# Security policy

## Supported versions

Only the latest release and the `main` branch get security fixes.

## Reporting a vulnerability

Please do not open a public issue for a security problem.

Report it privately through GitHub: <https://github.com/ferrislabs/rspigot/security/advisories/new>. Only the maintainers can read it.

Include what you can of:

- the component (server, plugin host, proxy, proxy plugin host) and the version or commit;
- how to reproduce it, ideally with a minimal client or packet capture;
- what an attacker gains: crash, denial of service, data loss, bypassing authentication or forwarding, running code.

We aim to answer within 7 days and to publish a fix and an advisory as soon as one is ready. We will credit you unless you ask us not to.

## Scope

In scope:
- anything a Minecraft client, or a peer on the plugin port or the proxy's plugin port, can do to the server or the proxy;
- forged Velocity forwarding;
- token checks on the plugin ports.

Out of scope:
- running the plugin ports on a public address without a token, which the server refuses and the proxy warns about;
- attacks that need write access to the configuration files.
