# Numera Emoji Mapper bots (Cloudflare Worker)

Both bots run inside one Worker, answer only the admins you list, and expose the
announcement endpoint that the local Rust publisher calls when a finished pack
is ready for your channel.

```
local build (Rust)  ──POST /publish (bearer)──►  Worker  ──Bot API──►  channel
                                                     ▲
                                    Telegram ────────┘  POST /tg/general
                                                        POST /tg/coin
```

## Read this before deploying

**A Telegram bot token can use `getUpdates` (polling) or a webhook: never
both.** The moment you register a webhook for a token, `numera-emoji emoji-bot` stops
receiving anything on it. That is not a bug and it is not this Worker's choice;
it is how the Bot API works. Run one or the other per token.

To hand a token back to the local poller, use the dedicated administration
command's webhook deletion mode for the intended bot (see Setup).

The Rust application owns pack creation, uploads and verified recovery of
ambiguous mutations. This Worker remains TypeScript and SQL; it reads updates
and sends messages.

## Routes

| Route | Auth | What |
|---|---|---|
| `POST /tg/general` | `X-Telegram-Bot-Api-Secret-Token` | Webhook for the general bot |
| `POST /tg/coin` | `X-Telegram-Bot-Api-Secret-Token` | Webhook for the coin bot |
| `POST /publish` | `Authorization: Bearer …` | Announce finished packs in the Pack Links channel |
| `POST /log-test` | `Authorization: Bearer …` | Send a live `🟢 Log channel test` entry to `LOG_CHAT_ID` |
| `GET /health` | none | Liveness plus boolean `log_db`, `log_channel`, and `pack_links` readiness. No secrets. |

Each bot has its **own** path and its **own** webhook secret. One shared secret
would mean a leak from either bot could forge updates for the other.

The `/publish` and `/log-test` Bearer scheme follows HTTP authentication syntax: scheme names
are case-insensitive and permit one or more separating spaces. The secret itself
must match exactly. An authentication failure returns a `WWW-Authenticate: Bearer`
challenge with its 401 response.

## The two directions

Send the bot **premium emoji** and it replies with their IDs. Send it **IDs**
and it replies with the emoji: one per line, comma-separated, `, `-separated,
or a single ID.

The whole message has to be IDs and separators for the reverse direction to
trigger. A long number inside a sentence is far more likely to be a chat ID or
a timestamp than something to look up.

IDs are resolved with `getCustomEmojiStickers` before anything is rendered.
A `<tg-emoji>` tag falls back to its placeholder glyph when the ID does not
exist, so rendering an unchecked ID makes a typo indistinguishable from a hit;
anything Telegram cannot resolve is named instead. Resolving also yields each
sticker's own emoji, which is the tag's fallback text: what shows up wherever
the custom emoji cannot be rendered (notification previews, copied text,
older clients), better than a column of identical stars. Viewing
custom emoji does not require Premium; only sending them does.

The Rust poller carries the same ID-list parsing and typed-ID reply behavior.
Shared contract fixtures keep its output aligned with the Worker.

## Logs

The Telegram log channel uses a structured HTML layout with six colored circle
orbs (`🔴🟠🟡🟢🔵🟣`), structured `key: <code>value</code>` fields, folded
`(@username)` annotations on numeric user and chat IDs, and a UTC footer:

```
🟢 Pack announced
bot: general
packs: 1
messages: 1
style: cards
pack_names: sample_by_bot
🕒 2026-11-27 12:00:00 UTC
```

```
🔴 ERROR · webhook
bot: general
update_id: 42
detail: update 42: sendMessage failed (400): chat not found
🕒 2026-11-27 12:00:00 UTC
```

Each orb maps to a distinct state:
- `🔴` errors and critical failures (`ERROR`, `CRITICAL`, `misconfigured`)
- `🟠` warnings and access denials (`WARNING`, `access.denied`)
- `🟢` pack announcements, log tests, and recovery (`publish`, `log_channel.test`)
- `🔵` emoji ID extraction and full reverse lookups (`emoji.lookup`, `emoji.reverse_lookup`)
- `🟡` greetings and partial lookups with unknown IDs (`bot.greeted`, `emoji.partial_lookup`)
- `🟣` allowed channel forwards and configuration events (`channel.forwarded`, `settings.changed`)

Every entry names the bot first (`bot: general` or `bot: coin`) because both
bots share this Worker, one log table, and one log channel. User-authored text
fields (`text`, `caption`, `message`, etc.) are never forwarded to the channel.

The channel is capped at **12 messages a minute**. Over budget it drops and
*counts* rather than queueing (a queue inside a Worker isolate outlives the
request it belongs to and loses the messages anyway), and the dropped count
rides on the next message that gets through. That budget is per isolate,
not global: it bounds one isolate handling a burst of redeliveries, not a fleet.

Field values are capped at 80 characters (400 characters for `error`, `detail`,
and `reason`) and messages at 1,500 characters, shortened cleanly without
splitting surrogate pairs or HTML tags. The UTC timestamp and any suppressed
count remain visible even when a field hits its length cap. A failed log sink
also writes a timestamped, redacted diagnostic to the console.

`LOG_CHAT_ID`, `PACK_LINKS_CHAT_ID`, and `BOT_ALLOWED_CHANNEL_IDS` all accept a
bare positive channel ID copied from the Telegram UI (`4211401345`) and add the
`-100` prefix the Bot API requires. For a log channel with multiple collectible
usernames, set `LOG_CHAT_ID` to its numeric `-100…` ID: webhook updates carry only
its primary `username`. Partial channel fan-out errors report delivered and failed
administrator counts without logging their IDs.

**Posts from the log channel and their automatic discussion mirrors are ignored.**
Both bots administer the channel, so each posted line returns as a `channel_post`.
Telegram also forwards channel posts into a linked discussion group as a
`message` or `edited_message`. Echo detection uses normalized channel-ID matching
(and case-insensitive `@username` matching); discussion mirrors additionally need
`is_automatic_forward: true` and a matching channel-type `sender_chat`. Detection
runs before authorization and logging so a mirrored log cannot generate another
access-denial log. Manual forwards, messages from other channels and unmatched
sources retain their normal authorization and handling.

**D1** stores **warnings, errors, and publishes only**. Routine handled updates
go to `console.log` (Workers Logs via `[observability]` in `wrangler.toml`) and,
when important, to the Telegram log channel without spending D1 writes.
Unauthenticated webhook-secret mismatches stay console-only (`store: false`,
`toChannel: false`) so external scanners cannot flood the log channel or exhaust
the account's daily D1 write quota (100,000 rows written, 5,000,000 read per day).

The table (`migrations/0002_lean_logs.sql`) has **no secondary index and no
AUTOINCREMENT**, so one stored line costs **1 row written and 0 read**. It is
capped at the **newest 5,000 rows**: once every 250 rows, keyed off the
`last_row_id` the insert just returned, one `DELETE … WHERE id <= ?` removes
older rows. Stored detail text is capped at 2,000 characters.

**Channel** (`LOG_CHAT_ID`) receives warnings, errors, critical failures, and
important business events (pack announcements, access denials, emoji lookups,
greetings, allowed channel forwards, and `/log-test` checks). Both bots must be
administrators of the channel so each posts its own lines.

Logging can never take the bots down: every sink failure is swallowed and
reported to `console`, which `wrangler tail` reads. `GET /health` reports
`log_db`, `log_channel`, and `pack_links` booleans so a missing binding or
secret is visible without waiting for a missing message.

```bash
npx wrangler d1 execute numera-emoji-mapper-logs --remote \
  --command "SELECT ts, bot, level, event, detail FROM logs ORDER BY id DESC LIMIT 20"
```

## Setup

First deploy for a new operator, in order:

1. `npx wrangler login`: once per machine, for your Cloudflare account.
2. `npx wrangler d1 create numera-emoji-mapper-logs`, then paste the printed
   `database_id` into `wrangler.toml`. The id there is the original
   installation's; a database belongs to one account.
3. `npx wrangler d1 migrations apply numera-emoji-mapper-logs --remote`
4. `numera-worker-admin secrets --env-file .env` from the repository root: the
   secrets from your local configuration (below).
5. `npx wrangler deploy`
6. `numera-worker-admin webhooks --base-url https://<your-worker>.workers.dev`
   from the repository root.

The administration tool is a dedicated Rust binary, separate from the Worker
TypeScript runtime. Build it explicitly from a source checkout:

```sh
cargo build --manifest-path runtime/Cargo.toml --locked --release --bin numera-worker-admin
runtime/target/release/numera-worker-admin secrets --env-file .env --dry-run
runtime/target/release/numera-worker-admin secrets --env-file .env
```

Run these examples from the repository root; use `.exe` on Windows. Below,
`numera-worker-admin` denotes that built executable. Install Worker dependencies
with `npm ci --prefix worker`, then run Wrangler commands from `worker/`.
Use `secrets --dry-run` to inspect selected key names without launching Wrangler
or writing generated values. The built command's `--help` lists all options.

The tool sends secret values to `wrangler secret put` through stdin. Values
must not appear in arguments, shell history or diagnostic output; diagnostics
identify key names and value lengths only.

Two configuration details require care:

- `ADMIN_USER_IDS` is **composed** from `PACK_OWNER_USER_ID` +
  `BOT_ALLOWED_USER_IDS`. The list fails closed, so a typo means the bots answer
  nobody and nothing tells you why.
- `GENERAL_WEBHOOK_SECRET`, `COIN_WEBHOOK_SECRET` and `PUBLISH_SECRET` are
  generated (32 random bytes) if `.env` has none: and written **back** to
  `.env`, because a second run that minted different ones would break every
  webhook delivery's secret check.

Doing it by hand instead:

```bash
wrangler secret put GENERAL_BOT_TOKEN
wrangler secret put COIN_BOT_TOKEN
wrangler secret put GENERAL_WEBHOOK_SECRET   # a long random string
wrangler secret put COIN_WEBHOOK_SECRET      # a DIFFERENT long random string
wrangler secret put PUBLISH_SECRET           # bearer for /publish
wrangler secret put ADMIN_USER_IDS           # e.g. 123456789,987654321
wrangler secret put PACK_LINKS_CHAT_ID       # "@yourchannel" or "-100…"
wrangler secret put BOT_ALLOWED_CHANNEL_IDS  # optional: "-100…,-100…"
```

The channel is a secret rather than a `[vars]` entry: not because it is a
credential, but because `wrangler.toml` is committed and that would publish the
channel name. Both arrive as `env.PACK_LINKS_CHAT_ID` at runtime, so it costs
nothing.

`ADMIN_USER_IDS` **fails closed**: unset, empty, or all-invalid means the bots
answer nobody. That is deliberate: a misconfiguration must not open the bots to
everyone. Only plain positive integers are accepted, so `0x10`, `12.5` and `1e3`
are ignored rather than silently coerced.

`BOT_ALLOWED_CHANNEL_IDS` lists the channels whose posts the bots answer. A
`channel_post` has no sender for `ADMIN_USER_IDS` to check, so without this list
anyone who made a bot administrator of their own channel could make it message
every admin. Unset or malformed means **no channel**. Set this optional Worker
secret explicitly with `wrangler secret put BOT_ALLOWED_CHANNEL_IDS`; it is
separate from the composed private-user allowlist.

Create the log database once, then deploy:

```powershell
npx wrangler d1 create numera-emoji-mapper-logs      # put the id in wrangler.toml
npx wrangler d1 migrations apply numera-emoji-mapper-logs --remote
npx wrangler deploy
```

Then point Telegram at it: one webhook per bot:

```sh
numera-worker-admin webhooks --base-url https://<your-worker>.workers.dev
numera-worker-admin webhooks --status
numera-worker-admin webhooks --delete --only general
```

Run administration commands from the repository root so they use the same local
configuration as the secret upload; `--env-file PATH` selects it explicitly.
`--only general` or `--only coin` selects one bot; `--only both` is the default. `--force` explicitly permits replacing a webhook that points elsewhere.
A matching deployed secret matters: Telegram can accept registration while every
subsequent delivery fails authentication. Status/delete modes should be reviewed
before changing an existing polling/webhook setup.

The bots must be **administrators** of both the pack-links channel and the log
channel to post in them.

## Announcing from the local build

Set these two in `.env` and the publishers route announcements through the
Worker instead of talking to Telegram themselves:

```
WORKER_PUBLISH_URL=https://<your-worker>.workers.dev/publish
WORKER_PUBLISH_SECRET=<the PUBLISH_SECRET you set above>
```

Leave either unset and the old direct path is used, unchanged. Both or neither:
a URL without a secret is a half-finished setup that would 401 every
announcement, so it takes the direct path rather than pretending to work.

All three Rust publishers (single pack, collection and coin family) use the
shared native announcer. Historical implementations had separate copies of the
routing logic; the retained cross-publisher contract prevents one workflow from
silently bypassing Worker routing.

The duplicate guard does not move: `state["sent"]` is what stops a re-run
announcing the same pack twice, and it holds whichever route sent it. A failed
announcement is **not** recorded as sent, so the next run retries it: and
neither route retries a `sendMessage` internally, because it is not idempotent
and has no dedup key, so a timeout after Telegram accepted the post cannot be
told from one before it.

A whole pack family goes in **one** `/publish` call, and the Worker splits it
across messages if it passes 4096 characters. A single message would be
rejected whole at that point, losing every link rather than just the overflow.

Body shape:

```json
{
  "bot": "coin",
  "note": "optional line above the list",
  "style": "cards",
  "packs": [{ "name": "cryptoemoji1_by_bot", "title": "Coins 1", "count": 200 }]
}
```

`style` picks the layout. **`cards`** (default) gives each pack a bold title
line with its link below: right for announcing one or two finished packs.
**`list`** is one line per pack, `title. url`, which is what a whole family of
29 reads as; as cards it is three screens of scrolling. It is explicit rather
than inferred from the pack count: the caller knows which it is announcing, and
a rule like "more than five means list" would silently change the look of a
real post. The direct (no-Worker) path renders the same two shapes, so turning
the Worker on cannot change how an announcement looks.

**Every field is checked before anything happens** (`src/validate.ts`). The
bearer proves *who* sent a body, never *what* is in it, and the TypeScript type
is only a compile-time claim about JSON nobody parsed. Three real defects went
through that gap: a `null` body threw `Cannot read properties of null` out of
the handler instead of answering 400; a misspelt `"genreal"` fell through the
`=== "general" ? … : "coin"` default and announced a general pack from the coin
bot's identity; and a string `count` reached the HTML Telegram parses.

Anything below is **400**, with a reason naming the field and the rule: and
nothing else happens: no token lookup, no message built, not even a log line
(which would itself be a `sendMessage` to the log channel).

| Field | Rule |
|---|---|
| the body | must be a JSON object: `null`, an array and a bare string are not |
| `packs` | a non-empty array, at most **500** entries |
| `packs[i]` | must be an object |
| `packs[i].name` | **required**, `[A-Za-z0-9_]{1,64}`: it becomes a public `t.me/addemoji/` link |
| `packs[i].title` / `.format` | absent, or a string of at most 4096 characters |
| `packs[i].count` | absent, or a non-negative safe integer: never a string |
| `bot` | absent (→ coin, the documented default), `"general"` or `"coin"` |
| `style` | absent (→ cards), `"cards"` or `"list"` |
| `note` | absent, or a string of at most 4096 characters |

`null` is refused rather than read as "unset": every caller sends real values, so
a `null` is a bug upstream and a loud 400 finds it. The `note` cap matters
because the note is one block and the renderer only splits *between* blocks: an
over-long one used to leave as a single message Telegram rejects, so half the
announcement landed and the rest was dropped mid-send. Empty or whitespace-only
notes are omitted so they cannot become a standalone empty Telegram message.
`packs` is bounded
because it is caller-shaped input, and unbounded work inside a Worker is a
request that never finishes. `test/publish-input.test.ts` covers each rule.

## Development

```bash
npm run typecheck     # tsc --noEmit
npm test              # Vitest suite, fetch stubbed
npm run dev           # wrangler dev
```

A local Worker needs its secrets too, or its admin list is empty and it answers
nobody. From the repository root,
`numera-worker-admin secrets --env-file .env --dev-vars` prepares the same keys
in git-ignored `worker/.dev.vars` for `npm run dev`, without printing values.
Confirm this mode in the built administration tool before using the new interface.

The tests stub `fetch`, so nothing in them can reach Telegram: the same rule
the guarded Rust suite enforces, for the same reason: a historical test once reached live
Telegram and replaced a sticker in a published pack.

The SQLite migration tests also cover upgrading populated legacy logs, delayed
evictions completing out of order, and recovery at the next eviction boundary
after a failed delete. They exercise the real migration SQL and logging path
against a local in-memory database.

## Errors never carry a credential

Every Bot API request URL embeds the bot token (`/bot<token>/sendMessage`), and
a transport failure names the URL it was attempting. That message travelled out
of `Telegram.call`, through the publish handler's catch, into the HTTP 502 body
**and** into every log sink: so a caller holding only the publish bearer could
be handed the bot token instead.

`src/redact.ts` is the one redactor, and everything on its way out goes through
it. Three independent rules, because each covers a hole the others do not:

| rule | catches |
|---|---|
| `/bot…/` URL shape | a token this Worker never had in `env`: another bot, a stale deployment, a token quoted inside an upstream description |
| bare `digits:secret` shape | a token outside a URL |
| the configured `env` literals | a secret with no recognisable shape, such as a webhook secret echoed back |

The credential env values are listed explicitly rather than derived by scanning
`env`: a binding added later must be considered rather than assumed harmless,
and a chat id or api base must stay readable in the messages that need it.

`errText` also flattens an error's `cause` chain before redacting: `String(err)`
hides a cause and `JSON.stringify` of an Error yields `{}`, so an unsanitised
cause is both invisible and liable to surface wherever something serialises it
more thoroughly. `logging.ts` keeps its own length cap, which is a storage-budget
concern and nothing to do with credentials.

`test/redaction.test.ts` asserts that a synthetic, correctly-shaped token appears
in no response body, no log line and no nested cause: while a successful
publish and an ordinary refusal still read normally.

## Behaviour worth knowing

- **Webhook handlers return 200 even when handling fails.** Telegram redelivers
  any non-2xx, and every action here is a `sendMessage`, so a redelivery after a
  partial success posts the same reply twice. Failures are logged instead.
- **Bot API success results are checked before use.** A malformed `sendMessage`
  result makes `/publish` return 502 instead of reporting an announcement with
  missing message IDs. A malformed emoji lookup is logged as a failure instead
  of telling the user that their IDs are unknown. Neither path retries the API
  call internally.
- **A stranger gets one reply in private and silence in a group**: answering in
  groups would make the bot a spam vector.
- **`custom_emoji_id` is shape-checked** before it reaches the HTML that
  Telegram parses. It is inbound data interpolated into a `tg-emoji` attribute;
  only decimal ids are rendered.
- **Long id lists are split** across messages under Telegram's 4096-character
  limit rather than being rejected.
