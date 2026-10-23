# Bruno feature inventory

Bruno's Electron main process exposes its whole backend to the renderer as IPC handlers, so the handler list is a feature inventory. Each group maps to the pawsman milestone that covers it ([design §5](../superpowers/specs/2026-10-23-pawsman-design.md#5-milestones)).

## Method

- Source: `usebruno/bruno`, default branch, shallow clone on 2026-10-23.
- Tool: `rea analyze-javascript-application <bruno> --artifact-format directory --integrity-policy record-and-continue` (rea 6.4.0).
- Result: 3600 files, 3002 parsed as JavaScript, 7 parse failures; 268 IPC operations, 264 main-process handlers, 263 literal channels, 4 dynamic channels.
- Channel names are the `channel` values in the evidence graph.

## Handlers by area

| Area | Channels (prefix `renderer:` unless shown) | pawsman |
| --- | --- | --- |
| Requests | `send-http-request`, `cancel-http-request`, `load-request`, `load-large-request`, `load-request-via-worker`, `save-request`, `save-multiple-requests`, `save-response-to-file` | M1 |
| Collection tree | `create-collection`, `open-collection`, `open-multiple-collections`, `mount-collection`, `mount-collection-v2`, `clone-collection`, `rename-collection`, `remove-collection`, `new-folder`, `new-request`, `clone-folder`, `delete-item`, `move-item`, `move-file-item`, `move-folder-item`, `move-item-cross-format`, `rename-item-name`, `rename-item-filename`, `resequence-items`, `save-collection-root`, `save-folder-root`, `update-bruno-config`, `ignore-folder`, `scan-for-bruno-files`, `add-collection-watcher`, `find-unique-folder-name` | M1 |
| Scratch and transient requests | `mount-workspace-scratch`, `save-scratch-request`, `save-transient-request`, `delete-transient-requests` | M1 |
| Environments and variables | `create-environment`, `save-environment`, `save-environment-extends`, `rename-environment`, `delete-environment`, `export-environment`, `update-environment-color`, global `*-global-environment*`, workspace `*-workspace-environment*`, `update-variable-in-file` | M1 |
| Dotenv | `create-dotenv-file`, `delete-dotenv-file`, `save-dotenv-raw`, `save-dotenv-variables`, workspace variants | M1 (`.env`), M3 (secrets) |
| Workspaces | `create-workspace`, `open-workspace`, `open-workspace-dialog`, `close-workspace`, `rename-workspace`, `export-workspace`, `import-workspace`, `load-workspace-*`, `add-collection-to-workspace`, `move-collection-to-workspace`, `reorder-workspace-collections`, `start-workspace-watcher`, `save-workspace-docs` | M1 (multi-collection sidebar) |
| UI state snapshots | `snapshot:get`, `snapshot:get-collection`, `snapshot:get-sidebar`, `snapshot:get-tabs`, `snapshot:save`, `update-ui-state-snapshot` | M1 (`.pawsman/` UI state) |
| Runner | `run-collection-folder`, `datastore:runner_responses:*` | M2 |
| Auth | `fetch-oauth2-credentials`, `refresh-oauth2-credentials`, `cancel-oauth2-authorization-request`, `is-oauth2-authorization-request-in-progress`, `clear-oauth2-cache` | M3 |
| Cookies | `add-cookie`, `modify-cookie`, `delete-cookie`, `delete-cookies-for-domain`, `get-parsed-cookie`, `create-cookie-string` | M3 |
| Proxy and TLS | `get-system-proxy-variables`, `refresh-system-proxy`, `browse-pac-file`, `refresh-pac-cache`, `clear-http-https-agent-cache`, `get-collection-security-config`, `save-collection-security-config` | M3 |
| GraphQL | `fetch-gql-schema`, `load-gql-schema-file` | M4 |
| WebSocket | `ws:start-connection`, `ws:send-message`, `ws:queue-message`, `ws:close-connection`, `ws:connection-status`, `ws:is-connection-active`, `ws:get-active-connections` | M4 |
| gRPC | `grpc:start-connection`, `grpc:send-message`, `grpc:end-request`, `grpc:cancel-request`, `grpc:load-methods-proto`, `grpc:load-methods-reflection`, `grpc:generate-sample-message`, `grpc:generate-grpcurl`, `grpc:get-active-connections` | M4 |
| SOAP | `resolve-wsdl-schema-ref` | Out of scope |
| Import and export | `import-collection`, `import-collection-zip`, `is-bruno-collection-zip`, `convert-postman-to-bruno`, `install-postman-packages`, `export-collection-postman`, `export-collection-zip`, `convert-to-json`, `migrate-collection-to-yml` | M5 (import only) |
| OpenAPI sync and drift | `fetch-openapi-spec`, `read-openapi-spec`, `save-openapi-spec`, `check-openapi-updates`, `compare-openapi-specs`, `apply-openapi-sync`, `get-collection-drift`, `get-endpoint-diff-data`, `add-missing-endpoints`, `delete-endpoints`, `reset-endpoints-to-spec`, `*-openapi-sync-config`, `swagger-fetch`, api-spec channels | M6 |
| Mock server | `mock-server-*` (21 channels: instances, routes, responses, start, stop, port, log) | M6 |
| Git | `clone-git-repository`, `connect-collection-to-git`, `disconnect-collection-from-git`, `get-collection-git-remote-url`, `list-remote-branches-for-url` | M7 |
| AI | `ai-autocomplete`, `ai-chat-stream`, `ai-generate-script`, `ai-generate-text`, `ai-stream-text`, `ai-test-provider`, `*-ai-api-key`, `get-ai-status` | M7 |
| Embedded terminal | `terminal:create`, `terminal:input`, `terminal:resize`, `terminal:kill`, `terminal:list-sessions` | Out of scope (pawsman runs in a terminal) |
| System monitor | `start-system-monitoring`, `stop-system-monitoring`, `is-system-monitoring-active` | Out of scope |
| Window and shell | zoom, fullscreen, devtools, window controls, `browse-*`, `show-in-folder`, `open-*` dialogs, `theme-change`, `save-preferences`, quit flow | TUI equivalents where meaningful (theme, preferences) |
