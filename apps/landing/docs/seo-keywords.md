# Harness SEO pages

Research date: 2026-09-27. These are qualitative keyword and search-intent findings, not paid keyword-tool volume or difficulty estimates. Do not assign numerical demand to them.

## Keyword map

| Path under /agents/ | Primary target | Supporting intent |
| --- | --- | --- |
| claude-code/ | Claude Code GUI | desktop client, session manager, remote sessions |
| cursor/ | Cursor CLI GUI | Cursor agent interface, workspace |
| codex/ | Codex GUI | Codex Linux client, multi-agent workspace |
| opencode/ | OpenCode desktop client | OpenCode GUI, remote access |
| antigravity/ | Antigravity agent client | Antigravity CLI GUI, ACP client |
| devin/ | Devin CLI client | Devin GUI, ACP client |
| hermes/ | Hermes Agent GUI | desktop client, repository sessions |
| pi/ | Pi coding agent GUI | Pi desktop app, session manager |
| grok/ | Grok Build GUI | Grok CLI desktop client |

Antigravity CLI GUI is a research query, not a claim that Zeron wraps the CLI. This integration uses Google's ACP server. Use the accurate agent-client wording in the page title.

## Search observations and sources

- Claude Code GUI / remote control searches surface the official desktop and mobile options: https://claude.com/download . Explain Zeron's shared workspace without claiming the vendor lacks an interface.
- Cursor CLI GUI searches surface its terminal agent and existing automation workflows: https://cursor.com/docs/cli/overview . Distinguish the agent client from the Cursor editor.
- Codex GUI / remote-session queries have both official products and third-party clients. https://developers.openai.com/codex/cli/ is the upstream setup reference. Target the concrete Linux and cross-harness use cases, not unsupported exclusivity claims.
- OpenCode remote-access intent is explicitly served by its server/web workflow: https://opencode.ai/v2/docs/cli/web . Do not equate Zeron device sync with attaching arbitrary OpenCode server URLs.
- Pi GUI searches surface dedicated clients: https://www.pi-gui.com/ and https://pi-desktop.com/ . Specify “coding agent” to avoid the unrelated consumer Pi chatbot.
- Hermes has its own desktop product: https://hermes-agent.nousresearch.com/docs/user-guide/desktop . Explain Zeron as an independent client; do not claim to expose Hermes gateways.
- Grok Build GUI searches surface ACP desktop clients: https://github.com/liaan/grok-desktop and https://github.com/technodweep/grok-build-gui . Distinguish coding-agent intent from grok.com chat and image generation.
- Devin CLI ACP queries surface https://github.com/openclaw/acpx/blob/main/agents/Devin.md . Target the CLI integration; no evidence here supports a cloud-session import claim.
- Antigravity documentation distinguishes its CLI and extension surface: https://antigravity.google/docs/getting-started?tab=cli and https://antigravity.google/docs/ide/extensions . Repository implementation determines Zeron's actual connection.

## Product evidence

- `README.md`: local-only mode, no account required locally, optional sync, host availability, separate local/synced profiles.
- `crates/harness/src/lib.rs`: Claude stream-json, Codex app-server JSON-RPC, Cursor SDK shim, OpenCode HTTP/SSE.
- `crates/harness/src/acp/mod.rs`: Devin/Grok/Hermes ACP, community pi-acp requirement, Antigravity agy_acp_server.
- `crates/harness/src/install.rs`: agent installation requirements.
- Landing download section: macOS Apple silicon, Windows x64, Linux x64/ARM64; iOS coming soon.

## Publishing and maintenance

`seo/agents.mjs` is the editable source of page copy. `seo/generate.mjs` renders full HTML when Vite starts. Generated `agents/` files are ignored; the build includes all ten pages as HTML inputs. Pages reuse the homepage nav, gallery markup, fixed landscape footer, glyph field, download behavior, theme control, and motion. Agent-specific CSS extends the existing LP tokens for directory links, setup steps, and FAQs.

Each detail page has distinct title, description, H1, workflow, integration notes, setup and FAQ. Screenshots are labeled as the shared interface rather than harness-specific captures. No reviews, ratings, adoption counts, or keyword-stuffed hidden copy are added. FAQs remain visible HTML; structured data describes the WebPage and breadcrumbs without promising a rich result.

Canonical URLs and sitemap target https://zeron.sh. Deploying the preview does not publish those URLs to the main domain or submit a sitemap to a search engine. Production must publish this same dist directory for indexing. The preview deployment uses an X-Robots-Tag header to avoid competing with the canonical site.

After production publication, verify sitemap retrieval and inspect the agent URLs in Search Console. Use actual impressions and query data to refine page priorities. No search-volume access was available for this research.
