# Security policy

Security fixes go to the latest release and to `main`.

## Reporting a vulnerability

Please don't open a public issue for this. Send a private report through GitHub instead: <https://github.com/ferrislabs/rspigot/security/advisories/new>. Only the maintainers can see it.

It helps if you tell us which part is affected (server, plugin host, proxy or proxy plugin host) and on which version or commit, and how to reproduce it. A small client or a packet capture is ideal. Also say what an attacker gets out of it, whether that's a crash, data loss, getting past authentication or forwarding, or running code.

We try to reply within a week, and we publish a fix and an advisory once the fix is ready. We'll credit you unless you'd rather we didn't.

## What counts

We want to hear about anything a Minecraft client can do to the server or the proxy. The same goes for anyone who can reach the plugin port or the proxy's plugin port, about forged Velocity forwarding data, and about holes in the plugin token checks.

Running a plugin port on a public address without a token is not a vulnerability: the server refuses to start that way and the proxy logs a warning. Attacks that need write access to the configuration files are out of scope too.
