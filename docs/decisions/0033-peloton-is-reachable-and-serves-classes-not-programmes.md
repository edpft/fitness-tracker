# 0033 — Peloton is reachable, and serves classes but not programmes

**Date**: 2026-09-05

**Reverses** the standing assumption, repeated in `docs/handover-2026-09-03.md`
and in decision 0029's consequences, that *"Peloton has no official API and the
operator will not share credentials, so an asserted FTP is the only path"*.

**Scope**: the Peloton driven adapter — what it can read, and what it cannot.

## What was decided

**Peloton's API is reachable with the operator's own credentials, through an
Auth0 authorization-code flow with PKCE.** Not through `/auth/login`, which
returns `403 Access forbidden. Endpoint no longer accepting requests` regardless
of headers, and which is what this agent tested before wrongly concluding the
whole API was closed.

The flow, as `philosowaffle/peloton-to-garmin` performs it and as verified here
on 2026-09-05:

```text
GET  auth.onepeloton.com/authorize   ...PKCE challenge, state, nonce
     → scrape the _csrf cookie for /usernamepassword/login
POST auth.onepeloton.com/usernamepassword/login   ...credentials, connection=pelo-user-password
     → an HTML form carrying wa, wctx, wresult
POST auth.onepeloton.com/login/callback           ...that form
     → six redirects → members.onepeloton.com/callback?code=...
POST auth.onepeloton.com/oauth/token              ...code + verifier
     → access token (48h) and refresh token
```

**What it serves:**

- **Class content, in full.** `target_metrics` gives every interval as start and
  end offsets with a power zone, `segments` gives warm-up, ride and cool-down
  lengths, and `is_ftp_test` and `is_power_zone_class` are flags. Every riding
  class in Build tiles exactly — the zone plan accounts for 100% of the ride
  segment.
- **The performed record.** 423 workouts, 283 of them cycling, back to
  2023-12-21.
- **FTP, effect-dated.** `/api/me` carries `cycling_workout_ftp`, and each
  `20 min FTP Test Ride` in the record yields the number Peloton derived:
  average output × 0.95, confirmed against all six.

**What it does not serve: programme structure.** Which class belongs to which
microcycle and session is not available on any REST path tried, and the GraphQL
gateway refuses introspection. **The skeleton is transcribed by the operator and
everything else is read.**

## Why it matters

**Three conclusions in the record were built on the wrong assumption**, and two
of them were this agent's:

- The FTP question was withdrawn on 2026-09-03 on the grounds that a stored FTP
  has no need to serve until there is Peloton data to read. There is. The
  operator's ruling that day — *"it would be useful to know what my zones were at
  some point in time but we can defer that until we read data from Peloton"* —
  is now due rather than deferred.
- The FTP **assertion** route was taken off the build list for the same reason.
  It is no longer the only path, and probably not the right one: an asserted
  number is a bootstrap where a tested one exists.
- 0029's reasoning that a provider must be transcribed rather than fetched holds
  only for the *skeleton*. Class content is fetched, and is better than what a
  screenshot gave: the Peak transcription's six-minute cool-down was the class's
  own one minute plus a separate five-minute ride, which the transcription had
  silently merged.

## What it costs

**This is an unofficial flow and it will break.** `/auth/login` is already dead;
the Auth0 client id is a public constant lifted from another project's source,
and nothing obliges Peloton to keep any of it working. **It must fail loudly.**
A silent fallback to stale data would be worse than an error, because a
prescription derived from last month's FTP looks exactly like one derived from
this month's.

**Credentials are the operator's and stay on his machine.** `PELOTON_EMAIL` and
`PELOTON_PASSWORD` in a gitignored `.env`, read as environment, never logged and
never stored by this tool. The refresh token is a credential too and gets the
same treatment.

**Rate.** Sixteen class fetches and four pages of workouts is the whole of what
was needed. A poll loop is not, and an authentication endpoint is not a thing to
retry against.

## Consequences

- The Peloton adapter grows an authenticating client beside
  `infrastructure/src/peloton/mapping.rs`, behind a port declared in
  `application` like every other source.
- `Ftp`, `FtpProvenance` and `ZoneBand::watts_at` have a producer at last, and
  the provenance for a Peloton-derived value is `Estimated` — it is the
  twenty-minute protocol's 95%, which is arithmetic over a test rather than a
  measurement.
- A **session is one or more classes**. `mapping::session()` already returns
  `.classes()`, built for the FTP warm-up and test pair; the common case is a
  main class plus a `5 min Cool Down Ride`, which is the operator's most-ridden
  class at 116 rides and appears on 122 of his 153 riding days. `CyclingSession`
  holds one warm-up, one ride and one cool-down, and whether that flattening is
  lossy is not settled here.
