# Security Policy

## Secrets

Numera Emoji Mapper talks to the Telegram Bot API and (optionally) CoinMarketCap. All
secrets are read from environment variables or a local `.env` file and are
never hard-coded in source:

- `TELEGRAM_BOT_TOKEN`: crypto-coin bot token
- `GENERAL_BOT_TOKEN`: general (non-coin) bot token
- `CMC_API_KEY`: optional CoinMarketCap Pro API key
- `PACK_OWNER_USER_ID`: numeric Telegram user id (pack owner)
- `WORKER_PUBLISH_SECRET`: bearer the local build sends to the Worker's `/publish`
- `GENERAL_WEBHOOK_SECRET`, `COIN_WEBHOOK_SECRET`: per-bot secrets Telegram
  echoes on every webhook delivery, so the Worker can reject forged updates

Rules:

- Never commit `.env`. Use `.env.example` as a template.
- Never use a real credential as a test fixture. Rust credential/redaction
  contracts and repository secret scanning protect tracked files; the guarded
  test runner never loads an operator `.env`.
- Bot tokens must not reach stdout, stderr or a log file. The token is embedded
  in every request URL, so exception text is redacted before printing
  (`telegram::Telegram::safe`), and native run logging renders redacted panic
  and error output. Route new error diagnostics through `logging::redact` or
  `logging::RunLog`, with configured secrets registered before validation.
- A leaked bot token should be revoked immediately via @BotFather (`/revoke`),
  then update `.env`. Removing a secret from a file does **not** remove it from
  git history: rotate the credential as well.

## Supported application platforms

The application supports Linux and Windows on x64 and arm64. The local application and
its administration/contributor tools are Rust; the browser panel and Worker
retain their own JavaScript/TypeScript security boundaries. Keep package
integrity checks, loopback/Origin/token checks, webhook authentication and owned
process cleanup active when changing those components.

## Reporting a vulnerability

If you find a security issue, please open a
[private report](https://github.com/KiaroSama/Numera-Emoji-Mapper/security/advisories/new)
(or contact the maintainer) rather than filing a public issue. Include reproduction steps and
the affected file/command. Do not include real tokens in any report.
