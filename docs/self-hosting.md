# Self-hosting an always-on peer

## You almost certainly don't need this

> **outl sync needs no server.**
> Pair your laptop and your phone and they sync with each other, directly, over P2P.
> That is the normal way to use outl and it is complete on its own — no box to run, no container, nothing on this page.

Read that first, because "self-hosting" usually implies there *is* a hosted thing you're opting out of, and here there isn't one.
Sync is peer-to-peer ([RFC 0038](rfcs/0038-sync-transport-and-workspace-identity.md)): two devices converge when they can reach each other, and the only shared component in the whole system is the [relay](relay.md), which forwards already-encrypted bytes it cannot read — and which you can [replace with your own](#running-your-own-relay).

So this page is **one option among several**, not a step in the setup.

| How you use outl | What you run |
|---|---|
| One device | Nothing. It's a local-first outliner. |
| Two or more devices you actually use | `outl peer pair` between them. **This is the common case.** |
| Devices that are rarely awake at the same time | Optionally, what's on this page. |
| iCloud Drive / Syncthing / a shared folder instead of P2P | `transport = "file"` — see [config](config.md). No pairing, no container. |

## When it does earn its keep

P2P converges when two devices can reach each other, and sometimes they never do.
A laptop shut at 18:00 and a phone edited at 22:00 do not overlap, so the phone's ops sit there until the laptop opens again.
An always-on peer is a third device that is awake for both of them.

If that describes you, running `outl serve` in a container on a machine you control — a NAS, a VPS, an old laptop in a closet — buys two things:

- **Convergence without overlap.** Every device syncs with the box; the box has everything.
- **A real replica.** Not a backup of a projection — the op log itself, every op, replayable. Losing every other device costs you nothing.

And it is worth being precise about what it still is not:

> It is **not** a server your devices sync *through*.
> There is no account, no upload, no service that holds your graph and hands it back.
> The container is one more **peer**, exactly like your laptop and your phone, doing exactly what they do — with one difference: it never goes to sleep.

Nothing about adding it changes the others: they keep talking to each other directly, and if the box is off, they carry on.

---

## Quick start

```bash
git clone https://github.com/outlmd/outl
cd outl
docker compose build
```

Then, **on a device that already has your notes** (laptop, desktop — not the server):

```bash
outl peer pair --name laptop
# → prints a ticket; leave it running, it waits 120s for the join
```

And back on the server, **hand it that ticket**:

```bash
docker compose run --rm outl peer pair --ticket <ticket> --name server
docker compose up -d
```

A ticket runs 400 to 750 characters, so pasting it as an argument is workable but unpleasant.
`--ticket -` reads stdin instead, which is nicer over SSH and pipes straight from a clipboard:

```bash
pbpaste | docker compose run -i --rm outl peer pair --ticket - --name server   # macOS
wl-paste | docker compose run -i --rm outl peer pair --ticket - --name server  # Wayland
```

That is the whole setup.
`docker compose logs -f outl` shows the sync passes.

### Why the server joins, and never hosts

The direction of that pairing is load-bearing, and it is the one thing in this page that will silently ruin your graph if you get it backwards.

Pairing is asymmetric: **the joiner adopts the host's workspace id**, and the host keeps its own.
So if the fresh, empty server hosts and your laptop joins, your laptop adopts the *empty* workspace's identity and stops converging with the graph it has been writing to.

Run `outl peer pair` (host) on the device that has your notes.
Run `outl peer pair --ticket …` (join) on the server.
The command prints which one happened — `Joined the host's workspace (<id>)` is what you want to see on the server.

### Why the container's workspace is created `--bare`

`outl init` normally seeds a `templates/journal` page and today's journal.
On a replica, those are ops written under *this* device's actor, and pairing keeps the joiner's ops — so a seeded server pushes a second `templates/journal` into your graph, and two page nodes with one slug both project to `pages/templates/journal.md`.

The image runs `outl init --bare` instead, which writes the directory layout and a config and **no ops at all**.
You can use the flag outside Docker too, for exactly the same case.
Do not use it for a workspace you intend to write in directly — the journal template will be missing.

---

## What is actually running

`outl serve`, both halves ([`docs/cli.md` → `outl serve`](cli.md)):

| Half | What it does here | Turn it off with |
|---|---|---|
| **Sync supervisor** | Holds this device's iroh endpoint, so every paired peer converges continuously. | `--no-sync` |
| **File watcher** | Reconciles `.md` written into `/data` from outside into the op log. | `--no-watch` |

The sync half is the reason the container exists.
The file watcher is a bonus that is genuinely useful — `scp` a markdown file into the volume, or point a `git` checkout at it, and the content joins your graph — but if you have no use for it, `--no-watch` is cheaper and takes no per-actor write lock:

```yaml
command: ["serve", "--no-watch"]
```

### It replicates the op log, not the `.md`

The op log is the source of truth, and the container has all of it.
The `.md` files are a *projection*, and the sync path deliberately does not rewrite them: a peer edit lands as ops, and nothing on this box asks for those ops to be rendered back to disk.

That is fine — no content is missing, it is in `ops/` — but it does mean `cat /data/pages/foo.md` on the server can show you yesterday.
When you want the markdown itself current:

```bash
docker compose exec outl outl doctor --repair
```

`doctor` names every page whose `.md` is absent or drifted and writes it, under the [invariant 8](../CLAUDE.md) guard that refuses to overwrite a `.md` holding content the log never saw.
Run it after the first pair, when the whole graph is unprojected.

> If you want to *read* the notes on that box, `docker compose exec -it outl outl tui` opens the TUI against `/data`.

---

## The two volumes

Both matter, and they are separate on purpose.

```yaml
volumes:
  - outl-data:/data          # your notes
  - outl-state:/home/outl    # this device's identity
```

**`/data`** is the workspace: `pages/`, `journals/`, `ops/`, `.outl/`.
It is what every peer replicates.

**`/home/outl`** is what makes this container *the same device* every time it starts:

| Path | What it is | What losing it costs |
|---|---|---|
| `~/.outl/identity.key` | The iroh secret key. **It is this device's node id** — the string every peer has stored in its `peers.json`. | The container comes back as a stranger. Every device still lists the old node id and shows it offline; you re-pair all of them. |
| `~/.config/outl/` | The device store: actor bindings, machine id. | The box writes under a fresh actor, minting a new `ops-<ulid>.jsonl`. Harmless per se, but they accumulate. |

The device store lives **outside** the workspace by design, and that is not a container detail — it is why `ops-<actor>.jsonl` is per-device in the first place.
Two devices resolving one actor id both append to one file, and last-write-wins loses ops with no error anywhere.

> **Do not set `$OUTL_DEVICE_DIR` on this container.**
> It means "throwaway actor" and it moves the iroh identity with it, so exporting it rotates the node id and unpairs everything.
> The image uses `$HOME` and `$XDG_CONFIG_HOME` instead, which is why the volume is one path.

### Bind mounts instead of named volumes

Fine, and often what you want for backups:

```yaml
volumes:
  - /srv/outl/data:/data
  - /srv/outl/state:/home/outl
```

A bind mount arrives owned by whoever owns it on the host, and the container runs as uid 1000, so take ownership once:

```bash
sudo chown -R 1000:1000 /srv/outl
```

The entrypoint checks both paths on start and names that command if either is unwritable, rather than letting the failure surface later as a permission error from inside a reconcile.

> **Why not chown automatically?**
> That needs a root entrypoint that drops privileges, and `docker exec` bypasses the entrypoint — so `docker compose exec outl outl peer pair`, the documented way to add a device, would run as root and write `identity.key` and `peers.json` root-owned, locking the daemon out of its own identity.
> One `chown` on the host is the cheaper half of that trade.

---

## Networking

There are no ports to publish, and the compose file has no `ports:` section.
iroh binds an OS-assigned UDP port and reaches peers by [hole punching, falling back to a relay](relay.md) — nothing dials *in* to a fixed address.

**On Linux, use `network_mode: host`.**
Docker's bridge NAT rewrites the source port, so the address a peer is told to dial back is not the one packets actually leave from, and connections end up pinned to the relay instead of going direct.
It still *works* on bridge — you just pay latency and the relay's bandwidth for traffic that had no need to go there.

On Docker Desktop (macOS / Windows) host networking behaves differently; drop the line and accept relayed connections.

Outbound is all it needs: UDP to peers and to the relay, plus HTTPS to the relay for the initial handshake.
No inbound firewall rule.

### Running your own relay

The default relay (`use1-1.relay.avelino.outl.iroh.link`) never sees your notes — it forwards ciphertext it cannot read.
If you would rather not depend on it at all, run [`iroh-relay`](relay.md) and point the workspace at it:

```toml
# /data/.outl/config.toml
[sync]
transport = "iroh"
relay_url = "https://relay.example.com"
```

That file is part of the workspace, so the setting reaches every paired device.

---

## Day two

### The published image, and what its tags mean

Building from the repo (what the quick start does) is one option; the other is pulling a prebuilt image from GHCR.

| Tag | Points at | Use it when |
|---|---|---|
| `ghcr.io/outlmd/outl-server:latest` | The highest **stable** release. Never a pre-release, and never an older release published later as a hotfix. | You want releases and don't want to think about it. |
| `:1.2` | Newest `1.2.x`. | You want patches but not minor bumps. |
| `:1.2.3` | Exactly that release. | |
| `:dev` | The tip of `main`. | You want what is being worked on, and accept that it is. |
| `:sha-a1b2c3d` | One commit, forever. **The only tag that never moves.** | The setup you would rather upgrade on purpose. |

The beta releases that `release.yml` publishes for every push to `main` do **not** get their own image — they are tagged by a workflow token, and GitHub does not fire workflows for those, by design. `:dev` is the equivalent, one image per push rather than one per tag.

### Upgrading

Building locally:

```bash
git pull && docker compose build && docker compose up -d
```

Or, on a published tag:

```bash
docker compose pull && docker compose up -d
```

The volumes survive.
`stop_grace_period: 30s` is in the compose file so SIGTERM has time to land: the daemon releases its endpoint lease on the way out, and a lease left held locks every outl process on that device out of an endpoint.

### Adding another device

Pair it against **any** device already in the graph, including this one:

```bash
docker compose exec -it outl outl peer pair --name server
# → prints a QR and a ticket; join from the new device
```

Here the server hosts and the new device joins, which is the right direction now: the server is no longer empty, it carries your workspace id, and the joiner should adopt it.

Pairing while the daemon is running is expected and handled — the pairing endpoint borrows the route for the length of the handshake and closes it, and the daemon rebuilds its transport when `peers.json` changes.

**A laptop** joins from the ticket: `outl peer pair --ticket <ticket>`, or `--ticket -` to pipe it in.

**A phone** joins from the QR — the mobile app pairs by camera, so for a phone the QR *is* the route in; there is nothing to paste.
Which makes the width of your SSH window part of the setup:

> **The QR wants up to 101 columns, and an 80-column window is often not enough.**
> A ticket carries one entry per network address the host found, so a laptop with Tailscale and a few Docker bridges mints a much longer one than a bare VPS: 400 characters at the low end, 750 at the high end, which is 77 to 101 columns of QR.
> Past about two direct addresses it stops fitting a default terminal, and a QR wider than the terminal wraps.
> A wrapped QR is not a degraded QR — no camera will ever decode it, and nothing on screen says why.
> So `outl peer pair` measures the terminal first, and when the QR will not fit it prints one line naming the width it needed rather than burying the ticket under fifty lines of noise.

When that happens, widen the window and re-run — or render the QR somewhere with room, from the ticket it already printed:

```bash
pbpaste | outl peer qr -     # on your laptop, in a big window
```

`outl peer qr` takes any ticket, as an argument or on stdin, and prints nothing but the QR.
Unlike `pair` it always prints, warning on stderr instead, because a command asked for a QR and nothing else has no useful way to refuse.

> Error correction is `L` rather than the usual `M`, which buys about 8 columns.
> `M`'s redundancy is aimed at print — a creased page, a smudged label — and this code is on a screen being photographed from thirty centimetres away.

### Backups

Back up **both** volumes.

`/data` alone restores your notes.
`/home/outl` alongside it restores them *as the same device*, which means no re-pairing and no new actor.

The op log is append-only JSONL, so a file-level snapshot of a running container is safe to take — a torn tail is a truncated last line, and `outl doctor` reports it rather than silently reading past it.

### Health checks

The image ships none, on purpose.
Both obvious probes are actively harmful here:

- `outl workspace info` takes the per-actor write lock, loses it to the running daemon, and mints a fresh ephemeral `ops-<ulid>.jsonl` — every 30 seconds, forever.
- `outl peer status` binds an endpoint and fights the daemon for the device lease.

Liveness is the process. `outl serve` exits non-zero when its watcher dies, so `restart: unless-stopped` covers the case a probe would have caught.

### Logs

`RUST_LOG=info` names each reconcile and each sync pass.
`RUST_LOG=outl=debug,iroh=info` is the setting for a pairing that will not connect.

---

## Security

**A paired peer is a trusted peer.** Pairing grants full read/write on the graph; there is no per-device scope ([RFC 0155](rfcs/0155-peer-trust.md)).
Treat the pairing ticket as a credential — it is single-use and short-lived, but for those 120 seconds it is the graph.

**Notes on disk are plaintext.** Markdown and JSONL, readable by anything with filesystem access.
If the box is shared or the disk leaves your control, that is what full-disk encryption is for.
outl encrypts in transit, not at rest.

**If the machine is lost or compromised**, `outl peer revoke-all` on a device you still have rotates the workspace identity and drops every pairing; you re-pair the devices you keep.
It stops the lost device receiving anything *new* — it cannot take back the history that device already synced.

---

## Troubleshooting

**`no workspace at /data`, or the container exits immediately.**
Something is wrong with the volume, not with outl — the entrypoint creates the workspace on `serve`. Check `docker compose logs outl` and that `/data` is writable.

**Paired, but nothing arrives.**
Check the workspace ids match: `docker compose exec outl cat /data/.outl/workspace-id` against the same file on your laptop.
If they differ, the pairing went the wrong direction — see [Why the server joins](#why-the-server-joins-and-never-hosts). Re-pair with the server as the joiner.

**`peer status` says "another outl process holds this device's sync endpoint".**
Correct and expected: the daemon is holding it. That message means "unknown", not "offline".

**`.md` files look old.**
Expected — see [It replicates the op log, not the `.md`](#it-replicates-the-op-log-not-the-md). `outl doctor --repair`.

**A pile of `ops-<ulid>.jsonl` files.**
Something else on that machine is opening the workspace and losing the write-actor race, or the `/home/outl` volume is not persisting.
`outl doctor` names the ephemeral actors; check the volume first.

---

## See also

- [The problem with how the others do it](sync.md) — why sync is P2P at all
- [Relay & NAT traversal](relay.md) — what the one shared component can and cannot see
- [CLI](cli.md) — `outl serve`, `outl peer`, `outl doctor`
- [Configuration](config.md) — `[sync]`, `[storage]`, `[snapshot]`
- [outl doctor](doctor.md) — what `--repair` will and will not overwrite
