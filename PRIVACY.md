# Privacy Policy

_Last updated: 2026-05-06_

FOUNDATION is a desktop application that runs entirely on your own machine. There is **no FOUNDATION server, no FOUNDATION account, and no FOUNDATION cloud**. We — the project maintainers — never receive, see, or store your data.

This document describes what happens to your data when you use FOUNDATION.

## 1. Local-only by default

All data you create or import in FOUNDATION is stored **locally on your computer**:

- The knowledge base (ontology, individuals, properties, blackboards, tasks, conversations, attachments) lives in a single SQLite database on your disk.
- Default location:
  - **Windows:** `%USERPROFILE%\Documents\Foundation\FOUNDATION.db`
  - **macOS:** `~/Documents/Foundation/FOUNDATION.db`
  - **Linux:** `~/Documents/Foundation/FOUNDATION.db`
- Application logs are written to the OS-standard app-data directory (e.g. on macOS: `~/Library/Application Support/org.w3id.foundation/application.log`).
- Attachments (PDFs, images, files you add to entities) are kept inside your local FOUNDATION directory.

## 2. The only outbound integration: Anthropic (Claude API)

The single feature that sends data outside your machine is the **integrated AI chat**, and **only** when you configure it to use Anthropic's Claude API.

When you send a message in the chat with the Claude provider selected, the following is transmitted directly from your computer to Anthropic's API endpoint (`api.anthropic.com`):

- The text of your message.
- The conversation history of the current chat.
- The system prompt configured in FOUNDATION.
- Context that FOUNDATION attaches to the request, which may include:
  - Subconscious snippets (a small set of ontology entities ranked as relevant to your message).
  - Blackboard contents currently in view.
  - Tool/MCP definitions and the results of any tool calls the model decides to make.
  - If **Camera Vision** is enabled, a webcam frame captured at send time and/or its derived expression/body-language description.

Once the data leaves your machine, it is governed by **Anthropic's** terms and privacy policy, not by FOUNDATION:

- Anthropic Privacy Policy: <https://www.anthropic.com/legal/privacy>
- Anthropic Commercial Terms / Usage Policies: <https://www.anthropic.com/legal>

You provide your own Anthropic API key. Billing, retention, and data-handling for those requests are between you and Anthropic.

If you choose the **bundled local model** (`llama-cpp` runtime) instead of Claude, the chat runs entirely on your machine and **no data is sent to Anthropic or anyone else**.

## 3. Local MCP server

FOUNDATION exposes a Model Context Protocol (MCP) server on your loopback interface only:

- `http://127.0.0.1:47178/mcp`
- `https://127.0.0.1:47177/mcp` (self-signed TLS)

These ports are bound to `127.0.0.1` and are not reachable from the network. Other applications on the same machine (e.g. Claude Code, Claude Desktop) can connect to them and read/write your FOUNDATION data through MCP tools — exactly as if you used those tools yourself.

If you deliberately expose the MCP server to the internet (for example, with a tunnel such as `ngrok`), then any data the connected client requests will leave your machine. That is your decision and your responsibility.

## 4. Camera Vision

The Camera Vision feature is **opt-in** and disabled by default. When enabled:

- A webcam frame may be captured each time you send a chat message.
- The frame and/or a derived description is included in the context sent to the active AI provider.
- If the provider is the bundled local model, the image stays on your machine.
- If the provider is Claude, the image and/or description is transmitted to Anthropic as described in section 2.

You can disable Camera Vision at any time in the settings panel.

## 5. Product analytics (PostHog) — opt-in

FOUNDATION includes an **opt-in** product-analytics integration with [PostHog](https://posthog.com/) to help the maintainers understand how the app is used in aggregate (which features are exercised, where users get stuck) so they can prioritize improvements.

Key properties:

- **Off by default.** PostHog is initialized with `opt_out_capturing_by_default: true`. No events are sent unless you actively turn analytics on, either during the setup wizard or in the settings panel.
- **No user identification.** FOUNDATION never calls `posthog.identify()` or attaches an email, name, or stable user ID. Person profiles are configured as `identified_only`, so as long as we never identify, no person profile is created on PostHog's side.
- **No ontology data.** None of your knowledge-base content is captured: no entities, no class names, no property names, no individual IRIs, no chat messages, no attachments, no blackboard contents, no search queries, no API keys, no file paths.
- **Behavioral signals only.** The events sent describe interactions with the UI (e.g. which feature was opened, which action was clicked, anonymous app/version metadata that PostHog automatically attaches such as OS, app version, and a random anonymous device ID generated by the PostHog SDK).
- **No automatic pageview capture** (`capture_pageview: false`).
- You can disable analytics at any time in **Settings → Analytics**; subsequent events stop being captured.

The data, when sent, goes to PostHog Cloud (default endpoint `https://us.i.posthog.com`, configurable at build time). It is then governed by **PostHog's** terms and privacy policy, not FOUNDATION's: <https://posthog.com/privacy>.

## 6. Other telemetry

Beyond the opt-in PostHog integration described above, FOUNDATION does **not** include any other telemetry, tracking, fingerprinting, A/B testing, or automatic crash reporting.

The only network traffic FOUNDATION initiates on its own is:

- requests to the AI provider you configured, when you actively use the chat (section 2);
- PostHog events, only after you have explicitly opted in (section 5).

## 7. Updates

**Today:** FOUNDATION does not auto-update. New versions are published as installers on the [GitHub Releases page](https://github.com/danielterra/FOUNDATION/releases). When you download an installer from GitHub, that download is subject to GitHub's privacy policy.

**Coming soon:** once the app is properly code-signed and notarized, FOUNDATION will be published on the platform app stores (e.g. Microsoft Store, Mac App Store) and may receive automatic updates through those stores. When that happens, update checks and downloads are handled by the store itself and are governed by **the store operator's** privacy policy (Microsoft, Apple, etc.) — FOUNDATION does not run its own update server. This document will be revised before that mechanism ships.

## 8. Your data, your control

Because everything is local:

- **Export / backup:** copy `FOUNDATION.db` and your attachments directory.
- **Deletion:** quit the app and delete `FOUNDATION.db` (and, optionally, the application-data directory). There is no "delete my account" request to make — there is no account.
- **Portability:** the database is a standard SQLite file; your data is yours, in an open format.

## 9. Children

FOUNDATION is a general-purpose tool and is not directed at children under 13. The maintainers do not collect any data, so no special handling applies on our side. If you let a minor use FOUNDATION on your machine, the local-only rules above still apply.

## 10. Changes to this policy

If the data-handling behavior of FOUNDATION changes (for example, if a new AI provider integration is added), this file will be updated in the same commit that introduces the change, and the date at the top of this document will be revised. The git history of this file is the canonical changelog.

## 11. Contact

FOUNDATION is an open-source project released under the **GNU AGPL-3.0**. There is no support desk and no data controller in the GDPR sense, because no personal data ever reaches the project maintainers.

For questions, security reports, or privacy concerns:

- Open an issue or discussion at <https://github.com/danielterra/FOUNDATION>
- Or email the maintainer directly: <daniel_terra@icloud.com>
