<script lang="ts">
  import * as api from "../api";
  import { store } from "../store.svelte";
  import TrackList from "../components/TrackList.svelte";
  import type {
    ArtistSummary,
    ListeningDna,
    PlaylistSummary,
    TrackSummary,
    UserProfile,
    UserSearchHit,
  } from "../types";

  let { onBack }: { onBack: () => void } = $props();
  let topArtists = $state<ArtistSummary[]>([]);
  let topTracks = $state<TrackSummary[]>([]);
  let playlists = $state<PlaylistSummary[]>([]);
  let profile = $state<UserProfile | null>(null);
  let profileLoading = $state(true);
  let brokenImages = $state<Set<string>>(new Set());
  function onArtworkError(url: string | null | undefined) {
    if (!url || brokenImages.has(url)) return;
    brokenImages = new Set(brokenImages).add(url);
  }
  let profileError = $state<string | null>(null);
  let profileQuery = $state("");
  let userResults = $state<UserSearchHit[]>([]);
  let userSearchLoading = $state(false);
  let searchTimer: number | null = null;
  let loading = $state(true);
  let partialError = $state<string | null>(null);

  // Listening DNA is loaded separately from the rest: it is the slowest of
  // these calls (two 50-item pages) and the least essential, so it must not
  // hold the header and top lists behind it.
  let dna = $state<ListeningDna | null>(null);
  let dnaLoading = $state(true);
  let dnaError = $state<string | null>(null);

  const isOwnProfile = $derived(profile?.isCurrentUser !== false);

  $effect(() => {
    let cancelled = false;
    Promise.allSettled([
      api.getTopArtists(12),
      api.getTopTracks(10),
      api.getPlaylists(6, 0),
      api.getUserProfile(),
    ]).then((results) => {
      if (cancelled) return;
      if (results[0].status === "fulfilled") topArtists = results[0].value;
      if (results[1].status === "fulfilled") topTracks = results[1].value;
      if (results[2].status === "fulfilled") playlists = results[2].value;
      if (results[3].status === "fulfilled") profile = results[3].value;
      else if (results[3].status === "rejected") profileError = store.handleError(results[3].reason, false).message;
      const failure = results.slice(0, 3).find((result) => result.status === "rejected");
      if (failure?.status === "rejected") partialError = store.handleError(failure.reason, false).message;
      loading = false;
      profileLoading = false;
    });
    return () => { cancelled = true; };
  });

  $effect(() => {
    let cancelled = false;
    dnaLoading = true;
    api
      .getListeningDna()
      .then((result) => { if (!cancelled) dna = result; })
      .catch((error) => { if (!cancelled) dnaError = store.handleError(error, false).message; })
      .finally(() => { if (!cancelled) dnaLoading = false; });
    return () => { cancelled = true; };
  });

  async function loadProfile(username?: string) {
    profileLoading = true;
    profileError = null;
    try {
      profile = await api.getUserProfile(username);
      profileQuery = "";
      userResults = [];
    } catch (error) {
      profileError = store.handleError(error, false).message;
    } finally {
      profileLoading = false;
    }
  }

  function searchProfileInput() {
    if (searchTimer !== null) clearTimeout(searchTimer);
    const query = profileQuery.trim();
    if (query.length < 2 || query.startsWith("spotify:user:") || query.includes("open.spotify.com/user/")) {
      userResults = [];
      return;
    }
    searchTimer = window.setTimeout(async () => {
      userSearchLoading = true;
      try {
        userResults = (await api.searchUsers(query)).users;
      } catch {
        // Exact profile opening still works when the optional fuzzy operation
        // is unavailable for this account/region.
        userResults = [];
      } finally {
        userSearchLoading = false;
      }
    }, 300);
  }

  function submitProfile(event: SubmitEvent) {
    event.preventDefault();
    const query = profileQuery.trim();
    if (query) void loadProfile(query);
  }

  /** Spotify hands the profile accent back as a packed 0xRRGGBB integer. */
  function profileHex() {
    if (profile?.color == null) return null;
    const rgb = (profile.color >>> 0) & 0xffffff;
    return `#${rgb.toString(16).padStart(6, "0")}`;
  }

  // ---- Listening DNA radar ------------------------------------------------
  //
  // One series (this account's profile) across six axes, so this is deliberately
  // a single-hue chart: --accent for the shape, hairline neutrals for the web.
  // No categorical palette is involved, and none should be — colouring each axis
  // differently would encode identity that isn't in the data.
  //
  // The polygon is the glanceable shape; the labelled list beside it is the
  // precise read. A radar cannot be measured accurately by eye (unequal-area
  // distortion between adjacent axes is inherent to the form), so the numbers
  // are always present in text rather than only on hover.

  const RADAR = 100; // viewBox half-extent; the chart is 200x200 user units.
  const RADIUS = 74; // leaves room for the outermost ring's stroke.
  /** Web rings, as fractions of full scale. Four is enough to read position
   * against without the grid competing with the data. */
  const RINGS = [0.25, 0.5, 0.75, 1];

  function axisPoint(index: number, count: number, fraction: number) {
    // -90° so the first axis sits at the top; clockwise from there, matching
    // how the labels are read.
    const angle = (Math.PI * 2 * index) / count - Math.PI / 2;
    return {
      x: RADAR + Math.cos(angle) * RADIUS * fraction,
      y: RADAR + Math.sin(angle) * RADIUS * fraction,
    };
  }

  function polygon(fraction: number, count: number) {
    return Array.from({ length: count }, (_, index) => {
      const { x, y } = axisPoint(index, count, fraction);
      return `${x.toFixed(2)},${y.toFixed(2)}`;
    }).join(" ");
  }

  const dnaShape = $derived.by(() => {
    // Bound to a local first: `dna` is $state, so it reads through a getter and
    // TypeScript cannot carry the null-narrowing into the callback below.
    const axes = dna?.axes ?? [];
    if (!axes.length) return "";
    return axes
      .map((axis, index) => {
        const { x, y } = axisPoint(index, axes.length, axis.value / 100);
        return `${x.toFixed(2)},${y.toFixed(2)}`;
      })
      .join(" ");
  });

  /** Where an axis name goes, and how it should be anchored there. Anchoring
   * by side keeps the text from crossing the polygon: a label on the right
   * grows rightward, one on the left grows leftward, and the top/bottom pair
   * centre. */
  function axisLabel(index: number, count: number) {
    const { x, y } = axisPoint(index, count, 1.22);
    const centred = Math.abs(x - RADAR) < 1;
    return {
      x,
      // Nudge the top and bottom labels clear of the vertex they sit on.
      y: centred ? (y < RADAR ? y - 1 : y + 7) : y + 4,
      anchor: centred ? "middle" : x > RADAR ? "start" : "end",
    };
  }

  const dnaVertices = $derived.by(() => {
    const axes = dna?.axes ?? [];
    return axes.map((axis, index) => ({
      axis,
      ...axisPoint(index, axes.length, axis.value / 100),
    }));
  });
</script>

<div class="profile">
  <button class="back" onclick={onBack}>← Back</button>

  <header class="hero" style:--profile-tint={profileHex() ?? "transparent"} class:tinted={profileHex() !== null}>
    {#if (profile?.imageUrl ?? store.auth.avatarUrl) && !brokenImages.has(profile?.imageUrl ?? store.auth.avatarUrl ?? "")}
      <img src={profile?.imageUrl ?? store.auth.avatarUrl ?? undefined} alt="" onerror={() => onArtworkError(profile?.imageUrl ?? store.auth.avatarUrl)} />
    {:else}
      <span class="avatar"></span>
    {/if}
    <div class="identity">
      <span class="muted label">{isOwnProfile ? "Your profile" : "Public profile"}</span>
      <h1 class="truncate">{profile?.displayName ?? store.auth.displayName ?? store.auth.userId ?? "Spotify account"}</h1>
      <p class="muted truncate">{profile?.username ?? store.auth.userId}{isOwnProfile && store.auth.product ? ` · ${store.auth.product}` : ""}</p>
      {#if profile}
        <div class="counts">
          <span><strong>{profile.followingCount ?? profile.following.length}</strong> following</span>
          <span><strong>{profile.followersAvailable ? profile.followers.length : "Private"}</strong> visible followers</span>
          <span><strong>{profile.totalPublicPlaylistsCount ?? profile.publicPlaylists.length}</strong> public playlists</span>
        </div>
      {/if}
    </div>
    {#if !isOwnProfile}<button class="mine" onclick={() => loadProfile()}>My profile</button>{/if}
  </header>

  <form class="profile-search" onsubmit={submitProfile}>
    <label for="profile-user">Open a Spotify profile</label>
    <div><input id="profile-user" bind:value={profileQuery} oninput={searchProfileInput} autocomplete="off" placeholder="Name, username, user URI, or profile URL" /><button disabled={!profileQuery.trim() || profileLoading}>Open</button></div>
    {#if userSearchLoading}<small class="muted">Searching people…</small>{/if}
    {#if userResults.length}<div class="user-results" role="listbox" aria-label="Spotify users">{#each userResults as user (user.uri)}<button type="button" onclick={() => loadProfile(user.username)}>{#if user.imageUrl && !brokenImages.has(user.imageUrl)}<img src={user.imageUrl} alt="" loading="lazy" onerror={() => onArtworkError(user.imageUrl)} />{:else}<span class="result-avatar"></span>{/if}<span><strong>{user.displayName}</strong><small>{user.username}</small></span></button>{/each}</div>{/if}
  </form>

  {#if loading || profileLoading}<p class="muted">Loading profile…</p>{/if}
  {#if profileError}<p class="notice">Profile could not be loaded: {profileError}</p>{/if}
  {#if partialError}<p class="notice">Some account content could not be loaded: {partialError}</p>{/if}

  {#if isOwnProfile}
    <section class="dna">
      <div class="dna-head">
        <div>
          <h2>Listening DNA</h2>
          <p class="muted">
            Six traits derived from your top artists’ genre tags and your top tracks’ release dates.
          </p>
        </div>
        {#if dna && !dna.sparse}
          <span class="source-tag" title={dna.tagSource === "spotify+lastfm"
            ? `Genre tags from Spotify plus Last.fm, for ${dna.lastfmArtists} of ${dna.artistSample} artists`
            : "Genre tags from Spotify only — a Last.fm key in Settings adds community tags"}>
            {dna.tagSource === "spotify+lastfm" ? "Spotify + Last.fm" : "Spotify tags"}
          </span>
        {/if}
      </div>

      {#if dnaLoading}
        <p class="muted">Reading your listening history…</p>
      {:else if dnaError}
        <p class="notice">{dnaError}</p>
      {:else if dna?.sparse}
        <p class="muted">Not enough listening history yet — Spotify needs a few weeks of plays before it will report top artists and tracks.</p>
      {:else if dna}
        <div class="dna-body">
          <!-- One series, one hue. The polygon is the shape; the list to its
               right carries the numbers, because a radar is not measurable by
               eye. Both are fed from the same array, so they cannot disagree. -->
          <svg class="radar" viewBox="0 0 200 200" role="img" aria-label="Listening DNA radar chart. Values are listed beside it.">
            {#each RINGS as ring}
              <polygon class="web" points={polygon(ring, dna.axes.length)} />
            {/each}
            {#each dna.axes as axis, index}
              {@const end = axisPoint(index, dna.axes.length, 1)}
              {@const label = axisLabel(index, dna.axes.length)}
              <line class="spoke" x1={RADAR} y1={RADAR} x2={end.x} y2={end.y} />
              <!-- Labels wear the muted text token, not the series colour: the
                   accent belongs to the shape, and re-using it here would read
                   as though the words were data. -->
              <text class="axis-name" x={label.x} y={label.y} text-anchor={label.anchor}>{axis.label}</text>
            {/each}
            <polygon class="shape" points={dnaShape} />
            {#each dnaVertices as vertex (vertex.axis.id)}
              <circle class="vertex" cx={vertex.x} cy={vertex.y} r="4">
                <title>{vertex.axis.label}: {vertex.axis.value} — {vertex.axis.basis}</title>
              </circle>
            {/each}
          </svg>

          <ul class="dna-axes">
            {#each dna.axes as axis (axis.id)}
              <li>
                <span class="axis-label">{axis.label}</span>
                <span class="meter" aria-hidden="true"><i style:width={`${axis.value}%`}></i></span>
                <span class="axis-value">{axis.value}</span>
                <small class="muted">{axis.basis}</small>
              </li>
            {/each}
          </ul>
        </div>

        {#if dna.genres.length}
          <div class="dna-genres">
            <span class="muted small-label">Top genre tags</span>
            <div class="chips">
              {#each dna.genres as genre (genre.name)}
                <span class="chip-static">{genre.name}<i>{genre.weight}%</i></span>
              {/each}
            </div>
          </div>
        {/if}

        <p class="dna-note muted">
          Spotify withdrew the public audio-features endpoint (danceability, energy, valence),
          so these traits are inferred from genre tags, popularity and release dates rather than
          measured from the audio. They are Rustify’s own reading, not Spotify’s.
          Based on {dna.artistSample} top artists and {dna.trackSample} top tracks.
          {#if dna.tagSource === "spotify+lastfm"}
            Genre tags come from Spotify, plus Last.fm community tags for {dna.lastfmArtists}
            {dna.lastfmArtists === 1 ? "artist" : "artists"}.
          {:else}
            Genre tags come from Spotify alone, and it leaves them empty for many artists —
            adding an optional Last.fm API key in Settings fills those gaps.
          {/if}
        </p>
      {/if}
    </section>
  {/if}

  {#if profile}
    {#if profile.recentlyPlayedArtists.length}<section><h2>Recently played artists</h2><div class="tiles">{#each profile.recentlyPlayedArtists as artist (artist.uri)}<button onclick={() => api.loadContext(artist.uri)}>{#if artist.imageUrl && !brokenImages.has(artist.imageUrl)}<img src={artist.imageUrl} alt="" loading="lazy" onerror={() => onArtworkError(artist.imageUrl)} />{:else}<span class="tile-image"></span>{/if}<strong class="truncate">{artist.name}</strong></button>{/each}</div></section>{/if}
    {#if profile.publicPlaylists.length}<section><h2>Public playlists</h2><div class="tiles">{#each profile.publicPlaylists as playlist (playlist.uri)}<button onclick={() => api.loadContext(playlist.uri)}>{#if playlist.imageUrl && !brokenImages.has(playlist.imageUrl)}<img src={playlist.imageUrl} alt="" loading="lazy" onerror={() => onArtworkError(playlist.imageUrl)} />{:else}<span class="tile-image square"></span>{/if}<strong class="truncate">{playlist.name}</strong><small class="truncate">{playlist.ownerName ?? profile.displayName}</small></button>{/each}</div></section>{/if}
    {#if profile.following.length}<section><h2>Following</h2><div class="chips">{#each profile.following as item (item.uri)}<button onclick={() => item.uri.startsWith("spotify:artist:") && api.loadContext(item.uri)} disabled={!item.uri.startsWith("spotify:artist:")}>{item.name}</button>{/each}</div></section>{/if}
    {#if profile.showFollows && profile.followers.length}<section><h2>Followers</h2><div class="chips">{#each profile.followers as item (item.uri)}<button disabled>{item.name}</button>{/each}</div></section>{/if}
    {#if !profile.followingAvailable || (profile.showFollows && !profile.followersAvailable)}<p class="muted relation-note">Some follow lists are private or unavailable for this profile.</p>{/if}
  {/if}

  {#if isOwnProfile}
    {#if topArtists.length}
      <section>
        <h2>Your top artists</h2>
        <div class="artist-grid">
          {#each topArtists as artist (artist.uri)}
            <button class="card artist" onclick={() => store.run(() => api.loadContext(artist.uri))}>
              {#if artist.imageUrl && !brokenImages.has(artist.imageUrl)}
                <img src={artist.imageUrl} alt="" loading="lazy" onerror={() => onArtworkError(artist.imageUrl)} />
              {:else}
                <span class="art"></span>
              {/if}
              <span class="truncate title">{artist.name}</span>
            </button>
          {/each}
        </div>
      </section>
    {/if}

    <!-- TrackList rather than a bare <ol>: it brings row artwork, the like
         toggle, credits and queueing, all of which the hand-rolled list here
         lacked. Passing no contextUri is deliberate — the top-tracks endpoint
         is not a playable Spotify context, so rows play as an explicit URI
         list. -->
    {#if topTracks.length}<section><h2>Your top tracks</h2><TrackList tracks={topTracks} /></section>{/if}

    {#if playlists.length}<section><h2>Your library playlists</h2><p class="muted">Showing {playlists.length} recent library entries.</p><div class="chips">{#each playlists as playlist (playlist.uri)}<button onclick={() => store.run(() => api.loadContext(playlist.uri))}>{playlist.name}</button>{/each}</div></section>{/if}
  {/if}
</div>

<style>
  .profile { max-width: 880px; margin: 0 auto; padding: 20px 0 38px; }
  .back { color: var(--fg-dim); margin-bottom: 18px; }

  /* Hero. Spotify hands back a per-profile accent colour; used as a soft wash
     behind the header rather than as a fill, because it is an arbitrary hue
     with no contrast guarantee against the ink sitting on top of it. At these
     weights it tints without ever becoming the text's background. */
  .hero {
    position: relative;
    display: flex;
    align-items: center;
    gap: 22px;
    margin-bottom: 18px;
    padding: 26px 24px;
    overflow: hidden;
    border-radius: var(--r-lg);
    border: 1px solid var(--hairline);
    background: var(--glass);
    backdrop-filter: blur(var(--blur));
  }
  .hero.tinted::before {
    content: "";
    position: absolute;
    inset: 0;
    z-index: -1;
    background:
      radial-gradient(70% 130% at 12% 0%, color-mix(in srgb, var(--profile-tint) 38%, transparent), transparent 70%),
      linear-gradient(120deg, color-mix(in srgb, var(--profile-tint) 16%, transparent), transparent 60%);
  }
  .hero img, .avatar { width: 108px; height: 108px; border-radius: 50%; object-fit: cover; flex: none; background: linear-gradient(135deg,#b08046,#6b4226); box-shadow: 0 12px 32px rgba(0,0,0,.34); }
  .identity { display: flex; flex: 1; min-width: 0; flex-direction: column; gap: 2px; }
  h1 { margin: 2px 0; font-size: clamp(26px, 4vw, 38px); letter-spacing: -.4px; }
  .hero p { margin: 0; }
  .label { font-size: 11px; text-transform: uppercase; letter-spacing: .08em; }
  .counts { display: flex; flex-wrap: wrap; gap: 16px; margin-top: 10px; color: var(--fg-dim); font-size: 12px; }
  .counts strong { color: var(--fg); font-size: 15px; }
  .mine { align-self: flex-start; flex: none; padding: 8px 14px; border: 1px solid var(--hairline); border-radius: 999px; background: var(--glass-strong); }

  .profile-search { display: grid; gap: 6px; margin-bottom: 14px; }
  .profile-search label { color: var(--fg-dim); font-size: 11px; }
  .profile-search div { display: flex; gap: 8px; }
  .profile-search input { flex: 1; min-width: 0; padding: 9px 11px; border: 1px solid var(--hairline); border-radius: var(--r-sm); background: var(--glass); color: var(--fg); }
  .profile-search input:focus-visible { outline: none; box-shadow: var(--focus-ring); }
  .profile-search button { padding: 8px 14px; border-radius: var(--r-sm); background: var(--accent); color: var(--ink); font-weight: 700; }
  .profile-search button:hover:not(:disabled) { background: var(--accent-hover); }
  .profile-search button:disabled { opacity: .45; }
  .user-results { display: grid !important; grid-template-columns: repeat(2,minmax(0,1fr)); gap: 6px !important; padding: 8px; border: 1px solid var(--hairline); border-radius: var(--r-sm); background: var(--glass); }
  .user-results button { display: flex; align-items: center; gap: 9px; min-width: 0; padding: 7px 8px; text-align: left; border-radius: var(--r-sm); background: transparent; color: var(--fg); }
  .user-results button:hover { background: var(--glass-strong); }
  .user-results img,.result-avatar { width: 34px; height: 34px; border-radius: 50%; object-fit: cover; flex: none; background: var(--glass-strong); }
  .user-results span:not(.result-avatar) { display: flex; min-width: 0; flex-direction: column; }
  .user-results strong,.user-results small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .user-results small { color: var(--fg-dim); }

  section { margin-top: 14px; padding: 18px 20px; border: 1px solid var(--hairline); background: var(--glass); border-radius: var(--r-md); }
  h2 { margin: 0 0 10px; font-size: 15px; }
  section p { margin: 0; }

  .dna-head { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; margin-bottom: 16px; }
  /* Provenance, stated on the card rather than only in the footnote: which
     tag source was used changes how much the shape is worth trusting. */
  .source-tag { flex: none; padding: 5px 11px; border-radius: 999px; color: var(--fg-dim); background: var(--glass-strong); border: 1px solid var(--hairline); font-size: 10px; text-transform: uppercase; letter-spacing: .08em; white-space: nowrap; }
  .dna-head p { font-size: 12px; max-width: 62ch; }
  .dna-body { display: flex; align-items: center; gap: 28px; }

  /* Recessive web, one accent shape. Strokes are thin and the fill is well
     under half opacity so the rings stay readable through it. */
  /* `overflow: visible` plus the margin: the axis labels are placed past the
     outer ring, outside the 200-unit viewBox, so the box needs room around it
     rather than a larger viewBox (which would shrink the polygon to make space
     for text). */
  .radar { width: 216px; height: 216px; flex: none; margin: 4px 34px; overflow: visible; }
  .axis-name { fill: var(--fg-dim); font-size: 10px; font-family: inherit; letter-spacing: .02em; }
  .web { fill: none; stroke: rgba(255,241,224,.09); stroke-width: 1; }
  .spoke { stroke: rgba(255,241,224,.07); stroke-width: 1; }
  .shape { fill: color-mix(in srgb, var(--accent) 26%, transparent); stroke: var(--accent); stroke-width: 2; stroke-linejoin: round; }
  .vertex { fill: var(--accent); stroke: rgba(18,11,4,.85); stroke-width: 1.5; }

  .dna-axes { display: grid; flex: 1; min-width: 0; gap: 12px; margin: 0; padding: 0; list-style: none; }
  .dna-axes li { display: grid; grid-template-columns: minmax(74px, auto) 1fr auto; align-items: center; gap: 10px; }
  .axis-label { font-weight: 600; font-size: 13px; }
  .axis-value { color: var(--fg); font-size: 13px; font-variant-numeric: tabular-nums; }
  .meter { display: block; height: 5px; border-radius: 999px; background: rgba(255,241,224,.1); overflow: hidden; }
  .meter i { display: block; height: 100%; border-radius: 999px; background: var(--accent); }
  .dna-axes small { grid-column: 1 / -1; font-size: 11px; line-height: 1.4; }

  .dna-genres { margin-top: 20px; padding-top: 16px; border-top: 1px solid var(--hairline); }
  .small-label { display: block; margin-bottom: 8px; font-size: 10px; text-transform: uppercase; letter-spacing: .1em; }
  .chip-static { display: inline-flex; align-items: center; gap: 7px; padding: 6px 11px; border-radius: 999px; background: var(--glass-strong); font-size: 12px; }
  .chip-static i { color: var(--fg-dim); font-style: normal; font-variant-numeric: tabular-nums; }
  .dna-note { margin-top: 16px !important; font-size: 11px; line-height: 1.5; max-width: 78ch; }

  .chips { display: flex; flex-wrap: wrap; gap: 8px; }
  .chips button { padding: 7px 10px; border-radius: 999px; background: var(--glass-strong); font-size: 12px; }
  .chips button:disabled { cursor: default; }

  .artist-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(118px, 1fr)); gap: 14px; }
  .artist-grid .card { background: transparent; border-color: transparent; padding: 6px; }
  .artist-grid .card:hover { background: var(--glass-hover); border-color: var(--hairline); }

  .tiles { display: grid; grid-template-columns: repeat(5,minmax(0,1fr)); gap: 10px; }
  .tiles button { display: flex; min-width: 0; flex-direction: column; gap: 5px; text-align: left; }
  .tiles img,.tile-image { width: 100%; aspect-ratio: 1; object-fit: cover; border-radius: 50%; background: var(--glass-strong); }
  .tiles .square { border-radius: var(--r-sm); }
  .tiles small { color: var(--fg-dim); }

  .notice { padding: 11px 14px; border: 1px solid rgba(220,190,90,.3); border-radius: var(--r-sm); background: rgba(190,160,50,.14); }
  .relation-note { margin-top: 12px; }

  @media (max-width: 760px) {
    .dna-body { flex-direction: column; align-items: stretch; }
    .radar { align-self: center; margin: 4px 34px 16px; }
  }
  @media (max-width: 700px) {
    .hero { flex-wrap: wrap; }
    .hero img, .avatar { width: 84px; height: 84px; }
    .tiles { grid-template-columns: repeat(3,minmax(0,1fr)); }
    .user-results { grid-template-columns: 1fr; }
  }
</style>
