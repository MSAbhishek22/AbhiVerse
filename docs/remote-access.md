# Remote access — Abhiverse from anywhere, not just your Wi-Fi

Same-Wi-Fi was never a product limit. It was only ever about
*where the coordination API runs*: in the dev setup the API runs
on the laptop's LAN, so the phone had to be on that LAN to reach
it. The phone and the laptop do not talk to each other directly
for control — **both talk to one coordination API**. Point both
sides at one public API and distance stops mattering.

## The one rule

Both sides must use the **same API URL**:

- Laptop agent: `abhiverse-agent.exe --pair --api <url>` — the URL
  is stored in the pairing record and used for heartbeats,
  transfers, and sessions afterwards.
- Phone app: Settings → Server → the same URL.

Mismatched URLs mean the pairing claim lands on one API while
the phone polls another: nothing ever connects, and the failure
looks mysterious even though the cause is just the mismatch.

## What already works in code

- The agent speaks HTTPS for pairing, heartbeats, and transfers
  (TLS on its HTTP client) — a hosted API is https-only, and the
  controller refuses public http by design, so this is required.
- The app takes any https server URL from Settings, no rebuild.
- Pairing approval still gates everything, requests are signed,
  revocation is instant. Going remote changes the network path,
  not the security model.

## Path A — proper hosted (VPS + domain, recommended)

The `deploy/` bundle: Caddy (automatic HTTPS) → API, plus a
coturn TURN relay. Full bring-up, costs, firewall ports, and
backups are in `deploy/README.md`. Short version:

1. Point DNS at the host (`A` record for `$DOMAIN`).
2. Open the firewall: 80/tcp, 443/tcp, 3478 (udp+tcp), 5349/tcp,
   49152–65535/udp. Nothing else inbound.
3. `cp deploy/.env.example deploy/.env`, set `DOMAIN` and a
   generated `TURN_SECRET`.
4. `docker compose -f deploy/docker-compose.yml up -d --build`.
5. Verify: `curl https://$DOMAIN/health` returns `"status":"ok"`.
6. Point the phone (Settings → Server) and the agent
   (`--pair --api https://$DOMAIN`) at `https://$DOMAIN`.

Media on this path: peer-to-peer via STUN normally, with the
coturn relay as the authenticated fallback (1-hour credentials
minted at `/config/rtc`). This is the **only** path with a TURN
fallback, which is what strict NATs need for video.

## Path B — quick test, no VPS (Cloudflare Tunnel)

For proving the anywhere path before paying for a VPS. The
laptop stays the server; the tunnel just gives it a public
https address:

1. On the laptop, run the API on localhost as usual
   (`ABHIVERSE_BIND_ADDR` unset, i.e. `127.0.0.1:8080`).
2. Install `cloudflared` and run:
   `cloudflared tunnel --url http://localhost:8080`
   It prints a public URL like `https://xxx.trycloudflare.com`.
3. Phone: Settings → Server → that URL.
4. Laptop: `abhiverse-agent.exe --pair --api <that URL>`,
   approve at the terminal, then run the agent normally.

Honest limits of this path:

- The laptop must stay on while testing — it *is* the server.
- The trycloudflare URL changes on every tunnel restart, and a
  changed URL means re-pointing both sides (phone Settings, plus
  re-pairing the agent, since the pairing record stores the old
  URL). A stable URL needs a named tunnel, which needs a free
  Cloudflare account and your own domain.
- **No TURN fallback**: a tunnel cannot relay TURN's UDP. Video
  works via direct STUN peer-to-peer on most home networks, but
  strict NATs (hostels, corporate Wi-Fi, strict mobile CGNAT) may
  fail video on this path. Pairing, presence, and file transfer
  still work — they are plain HTTPS, not WebRTC.

## What works from anywhere, per path

| Capability | VPS path | Tunnel path |
|---|---|---|
| Pairing, presence, file transfer | Yes | Yes |
| Remote video (normal NATs) | Yes | Yes (STUN direct) |
| Remote video (strict NATs) | Yes (TURN relay) | No — no TURN |
| Remote input | Same as video | Same as video |

## The clock caveat, plainly

Signed requests allow 120 seconds of clock skew — that window
stays, because it is replay protection, not a bug. Phones and
laptops with automatic date/time (the default) stay within
seconds of each other, so this only bites when automatic time is
off. If you ever see the clock-sync hint in an error, turn on
automatic date/time on both devices. It is not widened on
purpose: a wider window is a bigger replay hole.

## Security notes for hosted use

- Never expose the API over plain http to the internet. The app
  refuses public http anyway; do not work around that.
- Pairing approval is the security boundary. Approve only devices
  you physically own, every time.
- Revoke a lost device immediately, from the phone or the laptop.
- On a VPS, keep the box updated and back up `state.json`
  (see `deploy/README.md`). Sessions and TURN state are
  ephemeral by design and need no backup.
