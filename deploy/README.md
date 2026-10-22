# Abhiverse staging deployment

One small VPS runs the whole coordination stack. Clients only
make outbound connections; media flows peer-to-peer, with coturn
as the authenticated fallback.

```
Android / Windows clients
        |  HTTPS (443)
        v
Caddy — TLS termination (automatic certificates), reverse proxy
        |
        v
abhiverse-api — pairing, presence, signalling, /config/rtc
        |            \
        |             coturn (3478 udp/tcp, 5349 tls,
        |             49152-65535 udp relay range)
        v
named volume /data — state.json (pairings, transfers), chunks/
```

Status: **prepared, not yet deployed.** The bundle was written
and syntax-checked on 2026-10-22; per the project roadmap the
first live bring-up is the hosted-staging milestone, after the
real-device validation run. Docker is not available in the
dev sandbox, so the image build is first exercised on the VPS.

## Cost (typical, check current pricing)

- VPS with 1 vCPU / 1–2 GB RAM: roughly **US $5–6 / month**
  (Hetzner CX22, DigitalOcean basic, or similar).
- Domain: ~US $10–15 / year. TLS certificates: free (Caddy /
  Let's Encrypt). TURN bandwidth is the only variable cost and
  is negligible at beta scale.

## Bring-up

1. Point DNS at the host: `A` record for `$DOMAIN`.
2. Open the firewall: 80/tcp, 443/tcp, 3478 (udp+tcp),
   5349/tcp, 49152–65535/udp. Nothing else inbound.
3. `cp deploy/.env.example deploy/.env`, set `DOMAIN` and a
   generated `TURN_SECRET` (`openssl rand -hex 32`).
4. `docker compose -f deploy/docker-compose.yml up -d --build`
5. Verify: `curl https://$DOMAIN/health` returns `"status":"ok"`;
   `docker compose logs -f api caddy coturn` shows no errors.
6. Build clients with
   `--dart-define=ABHIVERSE_API_BASE_URL=https://$DOMAIN`
   (the API mints 1-hour TURN credentials at `/config/rtc`;
   no secret ever ships in a client).

## Operations

- **Backups:** the state that matters is one file —
  `state.json` in the `abhiverse_data` volume (pairings +
  transfer metadata). Nightly copy off-box, e.g.
  `docker run --rm -v abhiverse_data:/d -v $PWD:/b alpine cp /d/state.json /b/state-$(date +%F).json`.
  Sessions and TURN state are ephemeral by design; nothing
  else needs backing up.
- **Health:** `/health` is public and cheap; point any uptime
  checker at it. `docker compose ps` shows container health.
- **Logs:** `docker compose logs` (API logs contain no keys,
  codes, or secrets — pairing codes appear only on the agent's
  own console).
- **Shutdown:** `docker compose down` stops coordination;
  existing peer connections are unaffected until they drop, and
  no new pairings/sessions can start. For an emergency stop of
  one device, revoke it (the API change ends its sessions).
- **Before external testers:** confirm TLS is valid, the
  firewall matches the list above, TURN credentials expire
  (check a minted credential is ~1 h), and the v0.5.2 security
  tests pass against the deployed build.
