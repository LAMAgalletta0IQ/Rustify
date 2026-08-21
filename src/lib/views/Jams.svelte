<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import * as api from "../api";
  import { EVENT_JAMS } from "../api";
  import { store } from "../store.svelte";
  import type { JamEventPayload, JamStatus } from "../types";

  let status = $state<JamStatus | null>(null);
  let joinId = $state("");
  let busy = $state(false);
  let events = $state<JamEventPayload[]>([]);
  let copied = $state(false);
  let unlisten: UnlistenFn | null = null;

  const MAX_EVENTS = 50;

  async function refresh() {
    try {
      status = await api.getJamStatus();
    } catch (e) {
      store.handleError(e);
    }
  }

  async function run(fn: () => Promise<unknown>) {
    busy = true;
    try {
      await fn();
      await refresh();
    } catch (e) {
      store.handleError(e);
    } finally {
      busy = false;
    }
  }

  function onCreate() {
    run(() => api.createJam());
  }

  function onRefreshSession() {
    run(() => api.refreshJam());
  }

  async function onJoin() {
    const id = joinId.trim();
    if (!id) return;
    joinId = "";
    await run(() => api.joinJam(id));
  }

  function onJoinKey(e: KeyboardEvent) {
    if (e.key === "Enter") onJoin();
  }

  function onLeave() {
    run(() => api.leaveJam());
  }

  function onAddTrack() {
    run(() => api.addTrackToJam());
  }

  function onToggleQueueControl() {
    const session = status?.session;
    if (!session) return;
    run(() => api.setJamQueueControl(!session.queueControlAllowed));
  }

  function onKick(memberId: string) {
    run(() => api.kickJamMember(memberId));
  }

  function onEnd() {
    run(() => api.endJam());
  }

  /** Copies the invite link so it can be pasted to whoever is joining. */
  async function onCopyInvite() {
    const url = status?.session?.joinUrl ?? status?.session?.joinUri;
    if (!url) return;
    try {
      await navigator.clipboard.writeText(url);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch (e) {
      store.handleError(e);
    }
  }

  /** Keep the feed readable when a payload embeds a whole state object. */
  function fmt(ev: JamEventPayload): string {
    const s = JSON.stringify(ev);
    return s.length > 220 ? `${s.slice(0, 220)}…` : s;
  }

  onMount(async () => {
    await refresh();
    unlisten = await listen<JamEventPayload>(EVENT_JAMS, (e) => {
      events = [...events.slice(-(MAX_EVENTS - 1)), e.payload];
      void refresh();
    });
  });

  onDestroy(() => {
    unlisten?.();
  });
</script>

<div class="jams">
  <h2>Jams</h2>
  <p class="muted">
    Social listening sessions, over Spotify's internal social-connect service.
    This is experimental: the endpoint paths ship as defaults but are internal
    and can change, in which case they are overridden from <code>jams.toml</code>
    in the app data dir rather than in code (see README_jams.md).
  </p>

  <div class="actions">
    <button class="btn-primary" disabled={busy} onclick={onCreate}>
      Create a jam
    </button>
    <span class="join">
      <span class="find">
        <input
          placeholder="Jam id or link"
          bind:value={joinId}
          onkeydown={onJoinKey}
        />
      </span>
      <button
        class="chip"
        disabled={busy || !joinId.trim()}
        onclick={onJoin}
      >
        Join
      </button>
    </span>
    <button class="chip" disabled={busy || !status?.session} onclick={onLeave}>
      Leave
    </button>
    <button class="chip" disabled={busy || !status?.session} onclick={onRefreshSession}>
      Refresh
    </button>
    <button
      class="chip"
      disabled={busy || !status?.session}
      onclick={onAddTrack}
      title="Adds the currently playing track to the jam"
    >
      Add current track
    </button>
    {#if status?.session?.isSessionOwner}
      <button class="chip" disabled={busy} onclick={onToggleQueueControl}>
        {status.session.queueControlAllowed ? "Lock participant queue" : "Allow participant queue"}
      </button>
      <button class="chip danger" disabled={busy} onclick={onEnd}>End jam</button>
    {/if}
  </div>

  {#if status}
    <div class="config muted">
      spclient endpoints: {status.spclientEndpoints} captured · pathfinder
      hashes: {status.pathfinderHashes} captured
    </div>
    {#if status.spclientEndpoints === 0}
      <p class="hint muted">
        No jam endpoints configured — create/join will fail with a clear
        configuration message. Restore the defaults by deleting the
        <code>[spclient_endpoints]</code> table from <code>jams.toml</code> in
        the app data dir (see README_jams.md).
      </p>
    {/if}
  {/if}

  {#if status?.session}
    <div class="session glass">
      <h3>Session</h3>
      <p>
        Jam <code class="mono">{status.session.id}</code>
      </p>
      <p class="muted">
        {status.session.members.length} member{status.session.members.length === 1 ? "" : "s"} ·{" "}
        {status.session.queue.length} track{status.session.queue.length === 1 ? "" : "s"} in queue
      </p>
      {#if status.session.joinUrl || status.session.joinUri}
        <p class="invite">
          <span class="muted">Invite</span>
          <code class="mono">{status.session.joinUrl ?? status.session.joinUri}</code>
          <button class="chip" onclick={onCopyInvite}>
            {copied ? "Copied" : "Copy"}
          </button>
        </p>
      {/if}
      <p class="muted">
        {status.session.isSessionOwner ? "You are the host" : "Participant"}
        · queue control {status.session.queueControlAllowed ? "allowed" : "host only"}
        {status.session.sessionType ? ` · ${status.session.sessionType}` : ""}
      </p>
      {#if status.session.members.length}
        <ul class="members">
          {#each status.session.members as member (member.id)}
            <li>
              {#if member.imageUrl || member.largeImageUrl}
                <img src={member.imageUrl ?? member.largeImageUrl ?? ""} alt="" />
              {/if}
              <span>
                <strong>{member.displayName ?? member.name ?? member.username ?? member.id}</strong>
                <small class="muted">
                  {member.isHost ? "Host" : "Participant"}
                  {member.isListening ? " · listening" : ""}
                  {member.isControlling ? " · controlling" : ""}
                </small>
              </span>
              {#if status.session.isSessionOwner && !member.isHost && !member.isCurrentUser}
                <button class="chip danger" disabled={busy} onclick={() => onKick(member.id)}>Kick</button>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
      {#if status.session.queue.length}
        <ul class="queue">
          {#each status.session.queue as t (t.uri)}
            <li>{t.name ?? t.uri}</li>
          {/each}
        </ul>
      {/if}
    </div>
  {/if}

  {#if events.length}
    <h3 class="sec">Live events</h3>
    <ul class="feed">
      {#each events as ev (fmt(ev))}
        <li class="muted mono">{fmt(ev)}</li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .jams {
    padding-bottom: 12px;
  }
  h2 {
    font-size: 22px;
    margin: 26px 0 6px;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    margin: 18px 0 6px;
  }
  .join {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .join .find input {
    width: 220px;
  }
  .config {
    font-size: 12px;
    margin: 4px 0 14px;
  }
  .hint {
    font-size: 13px;
    max-width: 56ch;
    margin: 0 0 14px;
  }
  code {
    color: var(--fg);
    background: var(--glass-strong);
    border: 1px solid var(--hairline);
    border-radius: 6px;
    padding: 1px 6px;
    font-size: 12px;
  }
  .mono {
    font-family: "Cascadia Code", "Consolas", monospace;
    font-size: 12px;
    word-break: break-all;
  }
  .session {
    border-radius: var(--r-md);
    padding: 16px 18px;
    margin: 14px 0;
  }
  .session h3 {
    margin: 0 0 8px;
    font-size: 15px;
  }
  .session p {
    margin: 2px 0;
  }
  .invite {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin-top: 10px !important;
  }
  .queue {
    margin: 10px 0 0;
    padding-left: 18px;
    color: var(--fg-dim);
    display: grid;
    gap: 4px;
  }
  .members {
    list-style: none;
    margin: 14px 0 0;
    padding: 0;
    display: grid;
    gap: 8px;
  }
  .members li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px;
    border-radius: var(--r-sm);
    background: var(--glass);
  }
  .members img {
    width: 36px;
    height: 36px;
    border-radius: 50%;
    object-fit: cover;
  }
  .members span {
    flex: 1;
    min-width: 0;
    display: grid;
  }
  .members small {
    font-size: 11px;
  }
  .danger {
    color: #ff9b9b;
  }
  .feed {
    list-style: none;
    padding: 0;
    margin: 0;
    display: grid;
    gap: 6px;
  }
  .feed li {
    padding: 8px 12px;
    border-radius: var(--r-sm);
    background: var(--glass);
    border: 1px solid var(--hairline);
  }
</style>
