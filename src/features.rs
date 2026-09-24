//! In-page features: cosmetic feed ad CSS, cinema mode, hotkeys, Shorts hide,
//! prefer_hd, Discord RPC. SponsorBlock is handled by the real browser extension
//! loaded via `extension.rs`. In-video ad skipping is intentionally NOT done here.

/// Page-side JS: hides ad DOM elements (cosmetic CSS for feed ads, banners,
/// overlays) and reports play/pause state to Rust for SMTC (Now Playing).
/// Driven by a single `MutationObserver` rather than polling timers - YouTube's DOM only
/// changes when something actually happens, so there's no ongoing cost while
/// idle (e.g. paused on a static page).
///
/// `block_ads` false drops just the cosmetic ad-hiding CSS; `cinema` sets
/// whether dim-around-the-player starts enabled at launch (the player-bar
/// toggle is always injected so the user can flip it mid-session). `prefer_hd`
/// writes YouTube's local quality preference (1080p) before playback.
/// Media play/pause reporting and the keyboard shortcuts always run. Flags are
/// baked in at init-script build time (launch-time settings), so toggling
/// them takes effect on the next launch.
///
/// NOTE: this returns a `format!` string (to interpolate the flags), so every
/// literal `{`/`}` in the JS below is doubled to `{{`/`}}`.
pub fn script(block_ads: bool, cinema: bool, prefer_hd: bool) -> String {
    let block_ads_js = if block_ads { "true" } else { "false" };
    let cinema_js = if cinema { "true" } else { "false" };
    let prefer_hd_js = if prefer_hd { "true" } else { "false" };
    format!(
        r#"
    (function() {{
        var BLOCK_ADS = {block_ads_js};
        var CINEMA = {cinema_js};
        var PREFER_HD = {prefer_hd_js};

        // ---------- audio enhancement: equalizer + compressor ----------
        // Routing the <video> through a Web Audio graph lets us shape it with a
        // 5-band EQ and a gentle compressor (loudness normalization: quiet
        // dialogue comes up, sharp transients get tamed - good for cinema and
        // noisy mixes). Caveat: once a MediaElementAudioSourceNode is created
        // for an element it can't be removed, and the element's own
        // volume/muted stop reaching the output - so dedicated gains mirror
        // them back in (see the volumechange listener in trackVideoElement).
        var EQ_BANDS = [60, 230, 910, 3600, 14000];
        // Each preset: 5 band gains (dB) + its own compressor curve. `comp:
        // null` = transparent (ratio 1) - EQ colour without dynamics.
        // 'night' is the "bad speakers / late night" mode: hard limiting so
        // whispers and explosions land at the same level; 'voice' pushes
        // speech intelligibility for podcasts/interviews on laptop speakers.
        var GENTLE = {{ threshold: -22, knee: 28, ratio: 3.5, attack: 0.004, release: 0.30 }};
        var PRESETS = {{
            flat:   {{ gains: [0, 0, 0, 0, 0],   comp: null }},
            bass:   {{ gains: [9, 6, 1, -1, -2], comp: null }},
            vocal:  {{ gains: [-2, -1, 4, 3, 1], comp: GENTLE }},
            cinema: {{ gains: [5, 2, 0, 2, 4],   comp: GENTLE }},
            treble: {{ gains: [-2, -1, 0, 4, 8], comp: null }},
            voice:  {{ gains: [-6, -2, 5, 6, 0],
                       comp: {{ threshold: -28, knee: 20, ratio: 5, attack: 0.003, release: 0.25 }} }},
            night:  {{ gains: [-4, 0, 3, 2, -2],
                       comp: {{ threshold: -38, knee: 12, ratio: 12, attack: 0.002, release: 0.40 }} }},
        }};
        var PRESET_ORDER = ['flat', 'bass', 'vocal', 'cinema', 'treble', 'voice', 'night'];
        var PRESET_LABEL = {{
            flat: 'Нейтрально', bass: 'Бас', vocal: 'Вокал',
            cinema: 'Кино', treble: 'Высокие',
            voice: 'Речь', night: 'Ночной',
        }};
        var audio = {{ ctx: null, src: null, bands: [], comp: null,
                       muteGain: null, master: null, attached: null }};

        function savedAudioPreset() {{
            try {{ return localStorage.getItem('lg-audio-preset') || 'off'; }}
            catch (e) {{ return 'off'; }}
        }}

        function buildAudioGraph(v) {{
            if (audio.attached === v) return;
            // A MediaElementAudioSourceNode is one-shot per element: a second
            // createMediaElementSource() throws. Guard so a re-attach after we
            // lost track never breaks sound entirely.
            if (v.dataset.lgAudioBound === '1') return;
            v.dataset.lgAudioBound = '1';
            var AC = window.AudioContext || window.webkitAudioContext;
            if (!AC) return;
            try {{ audio.ctx = new AC(); }} catch (e) {{ return; }}
            audio.src = audio.ctx.createMediaElementSource(v);
            audio.bands = EQ_BANDS.map(function (freq, i) {{
                var f = audio.ctx.createBiquadFilter();
                f.type = i === 0 ? 'lowshelf'
                       : i === EQ_BANDS.length - 1 ? 'highshelf'
                       : 'peaking';
                f.frequency.value = freq;
                f.Q.value = 1.0;
                f.gain.value = 0;
                return f;
            }});
            var node = audio.src;
            audio.bands.forEach(function (b) {{ node.connect(b); node = b; }});
            audio.comp = audio.ctx.createDynamicsCompressor();
            audio.comp.threshold.value = -22;
            audio.comp.knee.value = 28;
            audio.comp.ratio.value = 3.5;
            audio.comp.attack.value = 0.004;
            audio.comp.release.value = 0.30;
            node.connect(audio.comp);
            // muteGain mirrors video.muted; master mirrors video.volume. Once
            // the element is routed through the graph these properties stop
            // affecting output directly, so we reapply them here.
            audio.muteGain = audio.ctx.createGain();
            audio.muteGain.gain.value = v.muted ? 0 : 1;
            audio.comp.connect(audio.muteGain);
            audio.master = audio.ctx.createGain();
            audio.master.gain.value = v.volume;
            audio.muteGain.connect(audio.master);
            audio.master.connect(audio.ctx.destination);
            audio.attached = v;
        }}

        // Push a compressor curve into the live node; null flattens it to
        // transparency (ratio 1 = no gain reduction ever).
        function applyComp(settings) {{
            if (!audio.comp) return;
            var s = settings || {{ threshold: 0, knee: 0, ratio: 1, attack: 0.003, release: 0.25 }};
            audio.comp.threshold.value = s.threshold;
            audio.comp.knee.value = s.knee;
            audio.comp.ratio.value = s.ratio;
            audio.comp.attack.value = s.attack;
            audio.comp.release.value = s.release;
        }}

        function applyAudioPreset(name) {{
            try {{ localStorage.setItem('lg-audio-preset', name); }} catch (e) {{}}
            var v = document.querySelector('video');
            if (!v) return;
            if (name === 'off') {{
                // Can't destroy the graph (one-shot source node) - flatten it
                // to transparency instead. Audibly neutral.
                if (audio.bands.length) audio.bands.forEach(function (b) {{ b.gain.value = 0; }});
                applyComp(null);
                return;
            }}
            if (!audio.attached) buildAudioGraph(v);
            if (!audio.attached) return;
            var p = PRESETS[name] || PRESETS.flat;
            audio.bands.forEach(function (b, i) {{ b.gain.value = p.gains[i] || 0; }});
            applyComp(p.comp);
            if (audio.ctx && audio.ctx.state === 'suspended') {{
                audio.ctx.resume().catch(function () {{}});
            }}
        }}

        function cycleAudioPreset() {{
            var cur = savedAudioPreset();
            var order = ['off'].concat(PRESET_ORDER);
            var idx = order.indexOf(cur);
            var next = order[(idx + 1) % order.length];
            applyAudioPreset(next);
            if (eqBtnRefresh) eqBtnRefresh();
            osdText(next === 'off' ? 'Звук: выкл' : ('Звук: ' + PRESET_LABEL[next]));
        }}

        // ---------- playback quality preference ----------
        // YouTube stores the user's manual quality pick in localStorage
        // (yt-player-quality). The IFrame API's setPlaybackQuality is a no-op
        // on embeds, but on youtube.com this pair still nudges the player
        // toward the chosen level. YouTube may still downshift on slow links.
        function applyQualityPref() {{
            if (!PREFER_HD) return;
            try {{
                var now = Date.now();
                localStorage.setItem('yt-player-quality', JSON.stringify({{
                    data: 'hd1080',
                    creation: now,
                    expiration: now + 2419200000
                }}));
            }} catch (e) {{}}
            var p = document.getElementById('movie_player');
            if (p && typeof p.setPlaybackQuality === 'function') {{
                try {{ p.setPlaybackQuality('hd1080'); }} catch (e) {{}}
            }}
        }}
        applyQualityPref();
        document.addEventListener('yt-navigate-finish', function () {{
            if (/^\/watch/.test(location.pathname)) applyQualityPref();
        }});

        // ---------- ad element hiding ----------
        // Feed/banner ads only — in-video ads are left to SponsorBlock.
        var adCss =
            "ytd-ad-slot-renderer,ytd-in-feed-ad-layout-renderer,ytd-banner-promo-renderer," +
            "ytd-promoted-sparkles-web-renderer,ytd-promoted-video-renderer,ytd-display-ad-renderer," +
            "ytd-companion-slot-renderer,ytd-measured-ad-layout-renderer,yt-measured-ad-layout-renderer," +
            "ytd-video-renderer[is-ad],ytd-compact-video-renderer[is-ad],ytd-reel-video-renderer[is-ad]," +
            "ytd-search-pyv-renderer,ytd-shopping-product-renderer[is-ad],#masthead-ad," +
            "ytd-engagement-panel-section-list-renderer[target-id=engagement-panel-ads]," +
            "ytd-rich-section-renderer:has(ytd-statement-banner-renderer)" +
            "{{display:none!important;}}";
        function addAdStyle() {{
            if (!BLOCK_ADS) return;
            if (document.getElementById('lg-adblock')) return;
            // This script runs at document-creation time, before the parser
            // has necessarily built <html>/<head> yet - both can be null
            // here. Bail out quietly; readystatechange/DOMContentLoaded
            // below guarantee a retry once a target actually exists.
            var target = document.head || document.documentElement;
            if (!target) return;
            var s = document.createElement('style');
            s.id = 'lg-adblock';
            s.textContent = adCss;
            target.appendChild(s);
        }}
        addAdStyle();
        document.addEventListener('readystatechange', addAdStyle);
        document.addEventListener('DOMContentLoaded', addAdStyle);

        // ---------- media play/pause reporting ----------
        var lastVideo = null;

        // ---------- cinema mode ----------
        // Spotlight veil: a transparent, pointer-transparent rect positioned
        // exactly over the player whose enormous box-shadow blacks out
        // everything AROUND it. This sidesteps z-index entirely - the earlier
        // attempt raised the player above a full-page overlay, but the player
        // can't escape YouTube's nested stacking contexts, so the overlay
        // swallowed the video too.
        var cinemaUserOn = CINEMA; // session override from the player toggle

        function addCinemaStyle() {{
            if (document.getElementById('lg-cinema-style')) return;
            var target = document.head || document.documentElement;
            if (!target) return;
            var s = document.createElement('style');
            s.id = 'lg-cinema-style';
            s.textContent =
                '#lg-cinema-veil{{position:fixed;z-index:2100;pointer-events:none;' +
                    'border-radius:18px;' +
                    'box-shadow:0 0 0 200vmax rgba(0,0,0,0.82);' +
                    'opacity:0;transition:opacity 0.7s cubic-bezier(0.22,1,0.36,1);}}' +
                'html.lg-cinema #lg-cinema-veil{{opacity:1;}}' +
                // The veil (2100) sits above the masthead (2020) and guide
                // drawer (2030); lift it while either is actually in use.
                'html:has(tp-yt-app-drawer#guide[opened],#masthead-container:focus-within) ' +
                    '#lg-cinema-veil{{opacity:0!important;}}' +
                '.lg-cinema-btn{{display:inline-flex!important;align-items:center!important;' +
                    'justify-content:center!important;}}' +
                '.lg-cinema-btn svg{{width:24px!important;height:24px!important;' +
                    'padding:0!important;flex-shrink:0;opacity:1;' +
                    'transition:opacity 0.2s ease;}}' +
                '.lg-cinema-btn[aria-pressed="false"] svg{{opacity:0.4;}}';
            target.appendChild(s);
        }}
        addCinemaStyle();
        document.addEventListener('readystatechange', addCinemaStyle);
        document.addEventListener('DOMContentLoaded', addCinemaStyle);

        function veilHost() {{
            return document.querySelector('#player-container-inner') ||
                   document.querySelector('.html5-video-player');
        }}

        function syncVeil() {{
            var veil = document.getElementById('lg-cinema-veil');
            if (!veil || !document.documentElement.classList.contains('lg-cinema')) return;
            var host = veilHost();
            if (!host) return;
            var r = host.getBoundingClientRect();
            veil.style.left = r.left + 'px';
            veil.style.top = r.top + 'px';
            veil.style.width = r.width + 'px';
            veil.style.height = r.height + 'px';
        }}
        window.addEventListener('resize', syncVeil);
        window.addEventListener('scroll', syncVeil, true);
        // Theater/default toggles and late layout shifts resize the player
        // without any window resize/scroll, which left the hole misplaced
        // (dimming the video itself) until the next 3 s poll.
        var veilResizeObserver = window.ResizeObserver ? new ResizeObserver(syncVeil) : null;
        var veilObservedHost = null;
        function observeVeilHost() {{
            if (!veilResizeObserver) return;
            var host = veilHost();
            if (host === veilObservedHost) return;
            if (veilObservedHost) veilResizeObserver.unobserve(veilObservedHost);
            veilObservedHost = host;
            if (host) veilResizeObserver.observe(host);
        }}

        function setCinema(on) {{
            if (!document.documentElement) return;
            var want = on && cinemaUserOn && /^\/watch/.test(location.pathname) &&
                       !document.fullscreenElement;
            if (want && !document.getElementById('lg-cinema-veil') && document.body) {{
                var veil = document.createElement('div');
                veil.id = 'lg-cinema-veil';
                document.body.appendChild(veil);
            }}
            document.documentElement.classList.toggle('lg-cinema', want);
            if (want) {{
                observeVeilHost();
                syncVeil();
            }}
        }}
        document.addEventListener('yt-navigate-finish', function () {{
            var v = document.querySelector('video');
            setCinema(!!(v && !v.paused));
        }});
        document.addEventListener('fullscreenchange', function () {{
            var v = document.querySelector('video');
            setCinema(!!(v && !v.paused));
        }});

        // Pretty toggle in the player's control bar (next to settings): a
        // moon icon that flips the session override. Dimmed icon = off.
        function ensureCinemaButton() {{
            var controls = document.querySelector('.ytp-right-controls');
            if (!controls) return;
            ensureAudioButton(controls);
            if (document.querySelector('.lg-cinema-btn')) return;
            var btn = document.createElement('button');
            btn.className = 'ytp-button lg-cinema-btn';
            btn.type = 'button';
            btn.title = 'Режим кинотеатра';
            btn.setAttribute('aria-label', 'Режим кинотеатра');
            btn.setAttribute('aria-pressed', String(cinemaUserOn));
            // Delhi-modern player icons are 24px SVGs centered in the 48px
            // button slot (not 100%-fill). Inline width/height="100%" made the
            // moon overflow the pill toggle background and clip on the right.
            var svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
            svg.setAttribute('viewBox', '0 0 24 24');
            svg.setAttribute('aria-hidden', 'true');
            var p = document.createElementNS('http://www.w3.org/2000/svg', 'path');
            p.setAttribute('fill', '#fff');
            // Material Design "dark_mode" crescent - a real cutout shape, not
            // two overlapping arcs (those render as a solid blob).
            p.setAttribute('d',
                'M12 3a9 9 0 1 0 9 9c0-.46-.04-.92-.1-1.36a5.389 5.389 0 0 1-4.4 2.26' +
                ' 5.403 5.403 0 0 1-3.14-9.8c-.44-.06-.9-.1-1.36-.1z');
            svg.appendChild(p);
            btn.appendChild(svg);
            btn.addEventListener('click', function (e) {{
                e.stopPropagation();
                cinemaUserOn = !cinemaUserOn;
                btn.setAttribute('aria-pressed', String(cinemaUserOn));
                var v = document.querySelector('video');
                setCinema(!!(v && !v.paused));
                osdText(cinemaUserOn ? 'Кинорежим включён' : 'Кинорежим выключен');
            }});
            controls.insertBefore(btn, controls.firstChild);
        }}

        // ---------- Discord RPC reporting ----------
        // Sends "rpc:<state>\x1f<id>\x1f<title>\x1f<channel>\x1f<pos>\x1f<dur>".
        // Deduped on state+id+title (NOT position - that changes every tick
        // and would spam Discord's ~15s rate limit); a seek detector forces a
        // resend when the playhead jumps off its expected course so the
        // progress bar stays honest after rewinds/skips.
        var lastRpcKey = '';
        var lastRpcPos = -1;
        var lastRpcAt = 0;
        function reportRpc(state) {{
            if (!window.ipc) return;
            var id = '';
            try {{
                var u = new URL(location.href);
                if (u.pathname === '/watch') id = u.searchParams.get('v') || '';
            }} catch (e) {{}}
            if (!id) state = 'none';
            var title = '', channel = '', pos = 0, dur = 0;
            if (state !== 'none') {{
                var tEl = document.querySelector('ytd-watch-metadata #title h1, h1.ytd-watch-metadata');
                title = tEl ? tEl.textContent.trim() : (document.title.replace(/ - YouTube$/, ''));
                var cEl = document.querySelector('ytd-watch-metadata #owner ytd-channel-name a, #channel-name a');
                channel = cEl ? cEl.textContent.trim() : '';
                var v = document.querySelector('video');
                if (v) {{
                    pos = Math.floor(v.currentTime) || 0;
                    dur = isFinite(v.duration) ? Math.floor(v.duration) : 0;
                }}
            }}

            var key = [state, id, title, channel].join('\x1f');
            var now = Date.now();
            var seek = false;
            if (state === 'play' && lastRpcPos >= 0 && key === lastRpcKey) {{
                var expected = lastRpcPos + (now - lastRpcAt) / 1000;
                seek = Math.abs(pos - expected) > 4;
            }}
            if (key === lastRpcKey && !seek) return;
            lastRpcKey = key;
            lastRpcPos = pos;
            lastRpcAt = now;
            window.ipc.postMessage('rpc:' + [state, id, title, channel, pos, dur].join('\x1f'));
        }}
        document.addEventListener('yt-navigate-finish', function () {{
            if (!/^\/watch/.test(location.pathname)) reportRpc('none');
        }});

        // Safety poll: autoplay fires 'play' before our listener attaches, so
        // event-only reporting misses the very first video. lastRpc dedupes,
        // so this only actually posts when something changed. Doubles as a
        // late-title fixup (the metadata renders after playback starts) and
        // keeps cinema mode honest for the same missed-event reason.
        setInterval(function () {{
            var v = document.querySelector('video');
            if (!v || !/^\/watch/.test(location.pathname)) {{
                reportRpc('none');
                setCinema(false);
                return;
            }}
            reportRpc(v.paused ? 'pause' : 'play');
            reportMedia(!v.paused);
            setCinema(!v.paused);
            syncVeil();
        }}, 3000);

        // SMTC status. The first autoplay usually starts before the listeners
        // below attach, which left the Now Playing widget "stopped" while
        // audio was playing; report on attach and from the poll, deduped.
        var lastMediaState = null;
        function reportMedia(playing) {{
            if (!window.ipc || lastMediaState === playing) return;
            lastMediaState = playing;
            window.ipc.postMessage(playing ? 'media:play' : 'media:pause');
        }}

        function trackVideoElement() {{
            var v = document.querySelector('video');
            if (!v || v === lastVideo) return;
            lastVideo = v;
            v.addEventListener('play', function () {{
                reportMedia(true);
                setCinema(true);
                reportRpc('play');
            }});
            v.addEventListener('pause', function () {{
                reportMedia(false);
                setCinema(false);
                reportRpc('pause');
            }});
            reportMedia(!v.paused);
            // Reattach the audio graph to the new element (SPA navigation can
            // swap the <video>), then reapply the saved EQ preset.
            audio.attached = null;
            var preset = savedAudioPreset();
            if (preset !== 'off') applyAudioPreset(preset);
            // video.volume/muted no longer reach the output once the graph is
            // attached, so mirror them through muteGain/master on every change.
            v.addEventListener('volumechange', function () {{
                if (audio.attached !== v) return;
                if (audio.master) audio.master.gain.value = v.volume;
                if (audio.muteGain) audio.muteGain.gain.value = v.muted ? 0 : 1;
            }});
        }}

        // ---------- fullscreen scroll lock ----------
        // YouTube's fullscreen can be class-based only (no Fullscreen API):
        // the player covers the viewport, but the document scrollbar stays
        // visible and the page behind it keeps scrolling. A lock class on
        // <html> forces overflow:hidden while ANY fullscreen mode is active
        // and the previous scroll position is restored on exit. Unconditional
        // - not gated by any setting flag.
        var fsLockActive = false;
        var fsLockSavedY = 0;

        function addFsLockStyle() {{
            if (document.getElementById('lg-fs-lock-style')) return;
            // Same early-document hazard as addAdStyle: head/documentElement
            // can still be null at document-creation time; the
            // readystatechange/DOMContentLoaded retries below guarantee a retry.
            var target = document.head || document.documentElement;
            if (!target) return;
            var s = document.createElement('style');
            s.id = 'lg-fs-lock-style';
            // scrollbar-gutter keeps the layout from shifting sideways when
            // the scrollbar disappears/reappears (classic scrollbars only;
            // a no-op with overlay scrollbars).
            s.textContent =
                'html.lg-fs-lock,html.lg-fs-lock body{{overflow:hidden!important;}}' +
                'html.lg-fs-lock{{scrollbar-gutter:stable;}}';
            target.appendChild(s);
        }}
        addFsLockStyle();
        document.addEventListener('readystatechange', addFsLockStyle);
        document.addEventListener('DOMContentLoaded', addFsLockStyle);

        // Every fullscreen path: the Fullscreen API, YouTube's
        // ytp-fullscreen class on #movie_player, or the fullscreen attribute
        // on ytd-watch-flexy (its own class-based mode).
        function fsLockWanted() {{
            if (document.fullscreenElement) return true;
            return !!document.querySelector(
                '#movie_player.ytp-fullscreen, ytd-watch-flexy[fullscreen]');
        }}

        function syncFsLock() {{
            if (!document.documentElement) return;
            var on = fsLockWanted();
            if (on === fsLockActive) return;
            fsLockActive = on;
            if (on) {{
                // Remember where the page was so exit lands back exactly -
                // YouTube itself may move the page while fullscreen.
                fsLockSavedY = window.scrollY || 0;
                document.documentElement.classList.add('lg-fs-lock');
            }} else {{
                document.documentElement.classList.remove('lg-fs-lock');
                // Restore in the same task as the unlock, before the next
                // paint, so there is no visible jump on exit either.
                var y = window.scrollY || 0;
                if (Math.abs(y - fsLockSavedY) > 1) window.scrollTo(0, fsLockSavedY);
            }}
        }}

        // Safety: SPA navigation and page teardown must never leave the
        // document stranded unscrollable. If a fullscreen mode is somehow
        // still active afterwards, the next mutation pass re-locks.
        function clearFsLock() {{
            fsLockActive = false;
            if (document.documentElement) {{
                document.documentElement.classList.remove('lg-fs-lock');
            }}
        }}
        document.addEventListener('yt-navigate-start', clearFsLock);
        document.addEventListener('yt-navigate-finish', clearFsLock);
        window.addEventListener('pagehide', clearFsLock);

        function onMutation() {{
            trackVideoElement();
            ensureCinemaButton();
            syncFsLock();
            // The consent lightbox renders long after DOMContentLoaded, so a
            // one-shot check at startup never found it.
            ensureConsentButtons();
        }}

        // The subtree observer fires hundreds of times per second on a busy
        // watch page; the handlers only need to run once per frame. Coalesce
        // mutation bursts into a single rAF-aligned pass.
        var mutationQueued = false;
        function onMutationThrottled() {{
            if (mutationQueued) return;
            mutationQueued = true;
            requestAnimationFrame(function () {{
                mutationQueued = false;
                onMutation();
            }});
        }}

        var observing = false;
        function ensureObserver() {{
            if (observing) return;
            // Same early-document-creation hazard as addAdStyle() above:
            // documentElement may still be null the first few times this runs.
            if (!document.documentElement) return;
            observing = true;
            new MutationObserver(onMutationThrottled).observe(document.documentElement, {{
                childList: true,
                subtree: true,
                attributes: true,
                attributeFilter: ['class'],
            }});
        }}
        ensureObserver();
        document.addEventListener('readystatechange', ensureObserver);
        document.addEventListener('DOMContentLoaded', ensureObserver);
        onMutation();

        // ---------- first-run consent fallback buttons ----------
        // If YouTube leaves the consent dialog without reachable buttons
        // (small window / rendering race on a cold profile), add plain DOM
        // buttons at the top of the dialog. They submit the same consent
        // endpoints YouTube itself uses, so this is a one-time first-launch fix.
        function ensureConsentButtons() {{
            var dlg = document.querySelector('ytd-consent-bump-v2-lightbox tp-yt-paper-dialog');
            if (!dlg || document.getElementById('lg-consent-buttons')) return;
            var row = document.createElement('div');
            row.id = 'lg-consent-buttons';
            row.style.cssText = 'display:flex;gap:12px;justify-content:flex-end;' +
                'padding:4px 4px 12px;position:relative;z-index:10;';
            function mk(label, primary) {{
                var b = document.createElement('button');
                b.textContent = label;
                b.style.cssText = 'all:unset;cursor:pointer;padding:10px 18px;' +
                    'border-radius:999px;font:500 14px Roboto,Segoe UI,sans-serif;' +
                    (primary ? 'background:#3ea6ff;color:#0d0d0d;' :
                                'background:rgba(255,255,255,0.10);color:#f1f1f1;');
                return b;
            }}
            var reject = mk('Отклонить', false);
            var accept = mk('Принять все', true);
            reject.addEventListener('click', function () {{
                document.cookie = 'SOCS=CAESHAgBEhJnd3NfMjAyMzA4MTAtMF9SQzIaAmRlIAEaBgiAo_CmBg;' +
                    ' Domain=.youtube.com; Path=/; Max-Age=33696000; SameSite=Lax';
                location.reload();
            }});
            accept.addEventListener('click', function () {{
                document.cookie = 'SOCS=CAESEwgDEgk0ODE3Nzk3MjQaAmVuIAEaBgiA_CWjBg;' +
                    ' Domain=.youtube.com; Path=/; Max-Age=33696000; SameSite=Lax';
                location.reload();
            }});
            row.appendChild(reject);
            row.appendChild(accept);
            dlg.insertBefore(row, dlg.firstChild);
        }}
        ensureConsentButtons();
        document.addEventListener('DOMContentLoaded', ensureConsentButtons);

        // ---------- hide Shorts ----------
        // Shelves on home/search, guide entries (full + mini sidebar), chips.
        // Direct /shorts/ links get rewritten to the normal watch player.
        function addShortsStyle() {{
            if (document.getElementById('lg-noshorts')) return;
            var target = document.head || document.documentElement;
            if (!target) return;
            var s = document.createElement('style');
            s.id = 'lg-noshorts';
            s.textContent =
                'ytd-rich-shelf-renderer[is-shorts],' +
                'ytd-reel-shelf-renderer,' +
                'ytd-rich-section-renderer:has(ytd-rich-shelf-renderer[is-shorts]),' +
                'ytd-guide-entry-renderer:has(a[title="Shorts"]),' +
                'ytd-mini-guide-entry-renderer[aria-label="Shorts"],' +
                'yt-chip-cloud-chip-renderer:has(yt-formatted-string[title="Shorts"]),' +
                'ytd-video-renderer:has(a[href^="/shorts/"]),' +
                'ytd-grid-video-renderer:has(a[href^="/shorts/"])' +
                '{{display:none!important;}}';
            target.appendChild(s);
        }}
        addShortsStyle();
        document.addEventListener('readystatechange', addShortsStyle);
        document.addEventListener('DOMContentLoaded', addShortsStyle);

        function redirectShorts() {{
            var m = location.pathname.match(/^\/shorts\/([^/?]+)/);
            if (m) location.replace('https://www.youtube.com/watch?v=' + m[1]);
        }}
        redirectShorts();
        document.addEventListener('yt-navigate-finish', redirectShorts);

        // ---------- on-screen indicator (shared by volume / clipboard) ----------
        // Proper pill UI: SVG speaker icon + progress bar + % label (volume
        // mode), or just a text label (toast mode). Built entirely with
        // createElement - Trusted Types on youtube.com forbids innerHTML.
        var SVG_NS = 'http://www.w3.org/2000/svg';
        var ICON_VOL = 'M3 9v6h4l5 5V4L7 9H3zm13.5 3c0-1.77-1.02-3.29-2.5-4.03v8.05' +
            'c1.48-.73 2.5-2.25 2.5-4.02zM14 3.23v2.06c2.89.86 5 3.54 5 6.71' +
            's-2.11 5.85-5 6.71v2.06c4.01-.91 7-4.49 7-8.77s-2.99-7.86-7-8.77z';
        var ICON_MUTE = 'M16.5 12c0-1.77-1.02-3.29-2.5-4.03v2.21l2.45 2.45' +
            'c.03-.2.05-.41.05-.63zm2.5 0c0 .94-.2 1.82-.54 2.64l1.51 1.51' +
            'C20.63 14.91 21 13.5 21 12c0-4.28-2.99-7.86-7-8.77v2.06' +
            'c2.89.86 5 3.54 5 6.71zM4.27 3 3 4.27 7.73 9H3v6h4l5 5v-6.73' +
            'l4.25 4.25c-.67.52-1.42.93-2.25 1.18v2.06c1.38-.31 2.63-.95 3.69-1.81' +
            'L19.73 21 21 19.73l-9-9L4.27 3zM12 4 9.91 6.09 12 8.18V4z';
        var osd = {{ el: null, icon: null, path: null, label: null, barWrap: null, barFill: null, timer: null }};

        function ensureOsd() {{
            if (osd.el || !document.body) return !!osd.el;
            var el = document.createElement('div');
            el.id = 'lg-osd';
            el.style.cssText =
                'position:fixed;top:76px;left:50%;transform:translateX(-50%) translateY(-6px);' +
                'display:flex;align-items:center;gap:12px;' +
                'background:rgba(18,18,18,0.88);backdrop-filter:blur(14px);' +
                'border:1px solid rgba(255,255,255,0.10);color:#f5f5f5;' +
                'padding:10px 18px;border-radius:999px;' +
                'font:500 13px "Roboto","Segoe UI",sans-serif;' +
                'z-index:99999;pointer-events:none;opacity:0;' +
                'transition:opacity 0.25s cubic-bezier(0.22,1,0.36,1),' +
                'transform 0.25s cubic-bezier(0.22,1,0.36,1);' +
                'box-shadow:0 12px 40px rgba(0,0,0,0.55);';

            var svg = document.createElementNS(SVG_NS, 'svg');
            svg.setAttribute('viewBox', '0 0 24 24');
            svg.setAttribute('width', '18');
            svg.setAttribute('height', '18');
            var path = document.createElementNS(SVG_NS, 'path');
            path.setAttribute('fill', '#f5f5f5');
            path.setAttribute('d', ICON_VOL);
            svg.appendChild(path);

            var barWrap = document.createElement('div');
            barWrap.style.cssText =
                'width:130px;height:4px;border-radius:999px;background:rgba(255,255,255,0.16);overflow:hidden;';
            var barFill = document.createElement('div');
            barFill.style.cssText =
                'height:100%;width:0%;border-radius:999px;background:#f5f5f5;' +
                'transition:width 0.16s cubic-bezier(0.22,1,0.36,1);';
            barWrap.appendChild(barFill);

            var label = document.createElement('span');
            label.style.cssText = 'min-width:34px;text-align:right;font-variant-numeric:tabular-nums;';

            el.appendChild(svg);
            el.appendChild(barWrap);
            el.appendChild(label);
            document.body.appendChild(el);
            osd.el = el; osd.icon = svg; osd.path = path;
            osd.label = label; osd.barWrap = barWrap; osd.barFill = barFill;
            return true;
        }}

        function osdShow() {{
            // Anchor to the PLAYER, not the viewport: centered horizontally
            // over the video, a bit below its top edge - where the eye
            // already is. Fullscreen = player is the whole screen, so the
            // same math lands it top-center of the display.
            var player = document.querySelector('.html5-video-player');
            if (player) {{
                var r = player.getBoundingClientRect();
                osd.el.style.left = (r.left + r.width / 2) + 'px';
                osd.el.style.top = (r.top + Math.min(56, r.height * 0.08)) + 'px';
            }} else {{
                osd.el.style.left = '50%';
                osd.el.style.top = '76px';
            }}
            // While fullscreen, only descendants of the fullscreen element
            // are rendered - a body-parented OSD would be invisible. Reparent
            // into whichever container is currently live.
            var fsHost = document.fullscreenElement || document.body;
            if (osd.el.parentNode !== fsHost) fsHost.appendChild(osd.el);
            osd.el.style.opacity = '1';
            osd.el.style.transform = 'translateX(-50%) translateY(0)';
            if (osd.timer) clearTimeout(osd.timer);
            osd.timer = setTimeout(function () {{
                osd.el.style.opacity = '0';
                osd.el.style.transform = 'translateX(-50%) translateY(-6px)';
            }}, 1200);
        }}

        function osdVolume(vol) {{
            if (!ensureOsd()) return;
            osd.icon.style.display = '';
            osd.barWrap.style.display = '';
            osd.path.setAttribute('d', vol === 0 ? ICON_MUTE : ICON_VOL);
            osd.barFill.style.width = Math.round(vol * 100) + '%';
            osd.label.textContent = Math.round(vol * 100) + '%';
            osdShow();
        }}

        function osdText(text) {{
            if (!ensureOsd()) return;
            osd.icon.style.display = 'none';
            osd.barWrap.style.display = 'none';
            osd.label.textContent = text;
            osdShow();
        }}

        // ---------- EQ toggle button in the player's right control bar ----------
        // Slotted next to the cinema toggle so both extras sit together. Cycles
        // off/flat/bass/vocal/cinema/treble and shows the current name as a
        // tooltip; lit-up icon = an effect is active.
        var ICON_EQ = 'M10 20v-6h4v6h5v-8h3L12 3 2 12h3v8z'; // speaker up
        var ICON_EQ_OFF = 'M7 9v6h4l5 5V4l-5 5H7zm9.5 3c0-1.77-1.02-3.29-2.5-4.03v8.05' +
            'c1.48-.73 2.5-2.25 2.5-4.02z';
        var eqBtnRefresh = null; // lets the panel repaint the toolbar icon

        // Glass popup listing every preset; click applies instantly. Lives
        // inside the player element so it survives fullscreen. Rebuilt lazily
        // (the player node can be replaced across SPA navigations).
        function toggleEqPanel(anchorBtn) {{
            var old = document.getElementById('lg-eq-panel');
            if (old) {{ closeEqPanel(); return; }}
            var player = document.querySelector('.html5-video-player');
            if (!player) return;
            var panel = document.createElement('div');
            panel.id = 'lg-eq-panel';
            panel.setAttribute('role', 'menu');
            panel.setAttribute('aria-label', 'Эквалайзер');
            panel.style.cssText =
                'position:absolute;right:12px;bottom:60px;z-index:9999;' +
                'display:flex;flex-direction:column;gap:2px;min-width:180px;' +
                'background:rgba(18,18,18,0.92);backdrop-filter:blur(14px);' +
                'border:1px solid rgba(255,255,255,0.10);border-radius:14px;' +
                'padding:8px;box-shadow:0 12px 40px rgba(0,0,0,0.55);' +
                'font:500 13px "Roboto","Segoe UI",sans-serif;color:#f5f5f5;';
            var cur = savedAudioPreset();
            var DESC = {{
                off: 'Без обработки', flat: 'Только компрессор выкл',
                bass: 'Глубокий низ', vocal: 'Чистый голос',
                cinema: 'Объём + динамика', treble: 'Яркий верх',
                voice: 'Речь, подкасты', night: 'Тихо и ровно',
            }};
            ['off'].concat(PRESET_ORDER).forEach(function (name) {{
                var row = document.createElement('button');
                row.type = 'button';
                var on = name === cur;
                row.setAttribute('role', 'menuitemradio');
                row.setAttribute('aria-checked', String(on));
                row.style.cssText =
                    'all:unset;display:flex;justify-content:space-between;gap:16px;' +
                    'cursor:pointer;padding:7px 12px;border-radius:9px;' +
                    (on ? 'background:rgba(255,255,255,0.14);' : '');
                var l = document.createElement('span');
                l.textContent = name === 'off' ? 'Выкл' : PRESET_LABEL[name];
                var d = document.createElement('span');
                d.textContent = DESC[name] || '';
                d.style.cssText = 'opacity:0.45;font-weight:400;';
                row.appendChild(l);
                row.appendChild(d);
                row.addEventListener('mouseenter', function () {{
                    row.style.background = 'rgba(255,255,255,0.10)';
                }});
                row.addEventListener('mouseleave', function () {{
                    row.style.background = on ? 'rgba(255,255,255,0.14)' : '';
                }});
                row.addEventListener('focus', function () {{
                    row.style.outline = '2px solid #a8d4ff';
                    row.style.outlineOffset = '-2px';
                }});
                row.addEventListener('blur', function () {{ row.style.outline = ''; }});
                row.addEventListener('click', function (e) {{
                    e.stopPropagation();
                    applyAudioPreset(name);
                    if (eqBtnRefresh) eqBtnRefresh();
                    closeEqPanel();
                    if (anchorBtn && anchorBtn.isConnected) anchorBtn.focus();
                    osdText(name === 'off' ? 'Звук: выкл' : ('Звук: ' + PRESET_LABEL[name]));
                }});
                panel.appendChild(row);
            }});
            panel.addEventListener('keydown', function (e) {{
                if (e.key !== 'Escape') return;
                e.preventDefault();
                e.stopPropagation();
                closeEqPanel();
                if (anchorBtn && anchorBtn.isConnected) anchorBtn.focus();
            }});
            player.appendChild(panel);
            if (anchorBtn) anchorBtn.setAttribute('aria-expanded', 'true');
            // Any click outside dismisses. Clicks on the toggle button are
            // left to its own handler: dismissing here first made the button
            // re-open the panel instead of closing it.
            eqDismiss = function (e) {{
                if (panel.contains(e.target)) return;
                if (anchorBtn && anchorBtn.contains(e.target)) return;
                closeEqPanel();
            }};
            document.addEventListener('click', eqDismiss, true);
            var first = panel.querySelector('[aria-checked="true"]') || panel.firstChild;
            if (first) first.focus({{ preventScroll: true }});
        }}

        var eqDismiss = null;
        function closeEqPanel() {{
            var panel = document.getElementById('lg-eq-panel');
            if (panel) panel.remove();
            if (eqDismiss) {{
                document.removeEventListener('click', eqDismiss, true);
                eqDismiss = null;
            }}
            var btn = document.querySelector('.lg-audio-btn');
            if (btn) btn.setAttribute('aria-expanded', 'false');
        }}

        function ensureAudioButton(controls) {{
            if (!controls || controls.querySelector('.lg-audio-btn')) return;
            var btn = document.createElement('button');
            btn.type = 'button';
            btn.className = 'ytp-button lg-audio-btn';
            btn.title = 'Эквалайзер';
            btn.setAttribute('aria-haspopup', 'menu');
            btn.setAttribute('aria-expanded', 'false');
            var svg = document.createElementNS(SVG_NS, 'svg');
            svg.setAttribute('viewBox', '0 0 24 24');
            svg.setAttribute('aria-hidden', 'true');
            var path = document.createElementNS(SVG_NS, 'path');
            path.setAttribute('fill', '#fff');
            svg.appendChild(path);
            btn.appendChild(svg);
            function refresh() {{
                var cur = savedAudioPreset();
                var active = cur !== 'off';
                path.setAttribute('d', active ? ICON_EQ : ICON_EQ_OFF);
                path.style.opacity = active ? '1' : '0.5';
                btn.title = cur === 'off' ? 'Эквалайзер (выкл)'
                    : ('Эквалайзер: ' + (PRESET_LABEL[cur] || cur));
                btn.setAttribute('aria-label', btn.title);
            }}
            eqBtnRefresh = refresh;
            btn.addEventListener('click', function (e) {{
                e.stopPropagation();
                toggleEqPanel(btn);
            }});
            refresh();
            controls.insertBefore(btn, controls.firstChild);
        }}

        // ---------- scroll-wheel volume over the player ----------
        // In fullscreen the wheel is captured EVERYWHERE: YouTube's new
        // fullscreen scrolls the page down to comments on wheel, which is
        // never what you want mid-video - volume is. Windowed mode keeps the
        // wheel-over-player requirement so feed scrolling works normally.
        // Scrollable player popups (quality/speed lists, the EQ panel,
        // chapters, end-screen overlays) must keep their own wheel scrolling.
        var WHEEL_PASSTHROUGH = '.ytp-popup, .ytp-settings-menu, .ytp-panel, #lg-eq-panel, ' +
            '.ytp-chapter-hover-container, .ytp-ce-element, .ytp-suggestion-set, ' +
            '.ytp-fullscreen-grid, .ytp-modern-videowall-still';
        document.addEventListener('wheel', function (e) {{
            // Pinch/ctrl+wheel is page zoom; a pure horizontal swipe used to
            // count as "wheel down" and silently lowered the volume.
            if (e.ctrlKey || e.deltaY === 0 || Math.abs(e.deltaX) > Math.abs(e.deltaY)) return;
            var player = document.querySelector('.html5-video-player');
            var v = document.querySelector('video');
            if (!player || !v) return;
            if (!document.fullscreenElement && !player.contains(e.target)) return;
            if (e.target && e.target.closest && e.target.closest(WHEEL_PASSTHROUGH)) return;
            e.preventDefault();
            e.stopPropagation();
            var step = e.deltaY < 0 ? 5 : -5;
            // Go through the player API when possible so YouTube's volume
            // slider, mute state and remembered volume stay in sync; writing
            // video.volume directly was overwritten by the player later.
            if (typeof player.getVolume === 'function' && typeof player.setVolume === 'function') {{
                var cur = player.getVolume();
                var next = Math.min(100, Math.max(0, Math.round(cur + step)));
                player.setVolume(next);
                if (next > 0 && player.isMuted && player.isMuted() && player.unMute) player.unMute();
                osdVolume(next / 100);
                return;
            }}
            var vol = Math.min(1, Math.max(0, v.volume + step / 100));
            v.volume = vol;
            if (vol > 0) v.muted = false;
            osdVolume(vol);
        }}, {{ passive: false, capture: true }});

        // ---------- Ctrl+Shift+C: copy link at current timestamp ----------
        document.addEventListener('keydown', function (e) {{
            if (!(e.ctrlKey && e.shiftKey) || e.altKey || e.metaKey) return;
            var lo = e.key.toLowerCase();
            // Ctrl+Shift+E - cycle the audio EQ preset (off/bass/vocal/...)
            if (lo === 'e' || lo === 'у') {{
                var vid = document.querySelector('video');
                if (vid) {{
                    e.preventDefault();
                    e.stopPropagation();
                    cycleAudioPreset();
                }}
                return;
            }}
            if (lo !== 'c' && lo !== 'с') return;
            var v = document.querySelector('video');
            var m = new URL(location.href).searchParams.get('v');
            if (!v || !m) return;
            e.preventDefault();
            e.stopPropagation();
            var t = Math.floor(v.currentTime);
            var url = 'https://youtu.be/' + m + (t > 0 ? '?t=' + t : '');
            navigator.clipboard.writeText(url).then(function () {{
                osdText('Ссылка с таймкодом скопирована');
            }}, function () {{
                osdText('Не удалось скопировать ссылку');
            }});
        }}, true);

        // ---------- fullscreen relay ----------
        // The player going fullscreen only fills the webview, not the OS
        // window - the native title bar would stay visible. Relay the state
        // so the Rust side can toggle real window fullscreen.
        document.addEventListener('fullscreenchange', function () {{
            if (window.ipc) {{
                window.ipc.postMessage('fs:' + (document.fullscreenElement ? '1' : '0'));
            }}
            // Lock/unlock the page scroll here too: fullscreenchange can fire
            // before any DOM mutation (or without one at all), so the
            // onMutation pipeline alone would miss the API-based path.
            syncFsLock();
        }});
        // A reload or navigation while fullscreen never delivers the matching
        // fullscreenchange, which stranded the OS window borderless without a
        // title bar. A fresh document is never fullscreen, so say so.
        if (window.ipc && !document.fullscreenElement) window.ipc.postMessage('fs:0');

        // ---------- app hotkeys ----------
        // Ctrl+H - home feed, Ctrl+L - focus the search box. Capture phase +
        // preventDefault so YouTube's own handlers (and Chromium's history
        // shortcut on Ctrl+H) don't fire.
        document.addEventListener('keydown', function (e) {{
            if (!e.ctrlKey || e.altKey || e.shiftKey || e.metaKey) return;
            var k = e.key.toLowerCase();
            if (k === 'h' || k === 'р') {{
                e.preventDefault();
                e.stopPropagation();
                location.href = 'https://www.youtube.com/';
            }} else if (k === 'l' || k === 'д') {{
                e.preventDefault();
                e.stopPropagation();
                var box = document.querySelector(
                    'ytd-searchbox input, input#search, form#search-form input');
                if (box) {{ box.focus(); box.select && box.select(); }}
            }}
        }}, true);

        // Alt+Left / Alt+Right - YouTube SPA history. history.back()/forward()
        // go through YouTube's own router, so the player element stays alive
        // (no full reload). Capture phase + preventDefault so Chromium's
        // native back/forward doesn't double-fire alongside it.
        document.addEventListener('keydown', function (e) {{
            if (!e.altKey || e.ctrlKey || e.shiftKey || e.metaKey) return;
            if (e.key === 'ArrowLeft') {{
                e.preventDefault();
                e.stopPropagation();
                history.back();
            }} else if (e.key === 'ArrowRight') {{
                e.preventDefault();
                e.stopPropagation();
                history.forward();
            }}
        }}, true);
    }})();
    "#
    )
}

/// Executed via `evaluate_script` when a hardware/keyboard media key is
/// pressed (relayed from SMTC on the Rust side).
pub fn media_command_script(action: &str) -> String {
    format!(
        r#"
        (function() {{
            var v = document.querySelector('video');
            switch ({action:?}) {{
                case 'play_pause':
                    if (v) {{ v.paused ? v.play() : v.pause(); }}
                    break;
                case 'next': {{
                    var next = document.querySelector('.ytp-next-button');
                    if (next) next.click();
                    break;
                }}
                case 'previous': {{
                    var prev = document.querySelector('.ytp-prev-button');
                    if (prev) {{ prev.click(); }}
                    else if (v) {{ v.currentTime = 0; }}
                    break;
                }}
            }}
        }})();
        "#
    )
}

