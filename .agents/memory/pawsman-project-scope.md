# Pawsman product scope

Pawsman is a pure-Rust ratatui API client inspired by Bruno, with no JavaScript or wasm. It uses its own on-disk format rather than Bruno-file compatibility. Kitty graphics, keyboard protocol and OSC 52 clipboard are first-class; it must work over SSH and degrade by probed capabilities on other terminals.

Locked scope decisions from October 23:

- Scripting: declarative captures/assertions with a Rhai escape hatch.
- Protocols: HTTP, GraphQL, WebSocket, gRPC and SSE.
- Importers: Bruno, Postman v2.1, OpenAPI 3/Swagger 2 and cURL.
- Features in scope: auth, cookies, proxy, collection runner and headless CLI, mock server, OpenAPI sync, Git and AI integration.

Follow the current program spec and repository rules for architecture, milestones and precise boundaries. Conventions were informed by semoxide and b24-chat-rs (formerly b24-experiment), especially rules/tui.md. Use rea for Bruno analysis as the user requested.

The original note called the repository private; the selected session later records the user making it public. Do not carry the old visibility claim forward as a current fact or put private infrastructure settings in the repository.

Source: [Claude memory](/home/smsteel/.claude/projects/-home-smsteel-src-github-pawsman/memory/pawsman-project-scope.md). Migrated 2026-10-30; scope: this project. Newer direct user instructions take precedence.
