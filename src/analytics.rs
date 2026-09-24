//! In-page video analytics panel, styled to match the monochrome theme.
//!
//! On watch pages, injects a stats card into the secondary column showing
//! views, likes, dislikes (read from the Return YouTube Dislike extension's
//! rendered counter), like/dislike ratio and engagement rate. Pure DOM
//! scraping - no extra network calls, so it works regardless of CSP.

pub fn script() -> &'static str {
    r#"
    (function() {
        var PANEL_ID = 'lg-analytics-panel';

        var css =
            '#' + PANEL_ID + '{' +
                'background:rgba(255,255,255,0.04);border-radius:16px;' +
                'padding:16px;margin-bottom:14px;color:#f5f5f5;' +
                'font-family:"Roboto","Segoe UI",sans-serif;' +
            '}' +
            '#' + PANEL_ID + ' .lg-an-title{' +
                'font-size:14px;font-weight:600;margin-bottom:12px;' +
                'display:flex;align-items:center;gap:8px;color:#f1f1f1;' +
            '}' +
            '#' + PANEL_ID + ' .lg-an-grid{' +
                'display:grid;grid-template-columns:1fr 1fr;gap:10px;' +
            '}' +
            '#' + PANEL_ID + ' .lg-an-cell{' +
                'background:rgba(255,255,255,0.05);border-radius:10px;padding:10px 12px;' +
            '}' +
            '#' + PANEL_ID + ' .lg-an-label{' +
                'font-size:11px;color:#b3b3b3;margin-bottom:3px;text-transform:uppercase;letter-spacing:0.4px;' +
            '}' +
            '#' + PANEL_ID + ' .lg-an-value{font-size:16px;font-weight:600;color:#ffffff;}' +
            '#' + PANEL_ID + ' .lg-an-bar{' +
                'grid-column:1 / -1;height:6px;border-radius:999px;overflow:hidden;' +
                'background:rgba(255,255,255,0.10);margin-top:2px;' +
            '}' +
            '#' + PANEL_ID + ' .lg-an-bar-fill{height:100%;background:#f1f1f1;border-radius:999px;' +
                'transition:width 0.4s cubic-bezier(0.22,1,0.36,1);}' +
            // Return YouTube Dislike's promotional "premium teaser" card would
            // otherwise show raw like/dislike counts above this panel.
            '.ryd-premium-teaser{display:none!important;}';

        function ensureStyle() {
            if (document.getElementById(PANEL_ID + '-style')) return;
            var s = document.createElement('style');
            s.id = PANEL_ID + '-style';
            s.textContent = css;
            (document.head || document.documentElement).appendChild(s);
        }

        function parseCount(text) {
            if (!text) return null;
            text = text.replace(/ | /g, ' ').toLowerCase();
            var m = text.match(/([\d][\d\s.,]*)\s*(тыс|млн|млрд|k|m|b)?/);
            if (!m) return null;
            var raw = m[1].replace(/\s/g, '');
            // "12,345" is digit grouping (en-US), "12,345 тыс" is a decimal
            // comma (ru). A comma before exactly 3 trailing digits groups;
            // anything else is a decimal separator.
            var num = /,\d{3}(\D|$)/.test(raw)
                ? parseFloat(raw.replace(/,/g, ''))
                : parseFloat(raw.replace(',', '.'));
            if (isNaN(num)) return null;
            var mul = { 'тыс': 1e3, 'k': 1e3, 'млн': 1e6, 'm': 1e6, 'млрд': 1e9, 'b': 1e9 }[m[2]] || 1;
            return Math.round(num * mul);
        }

        function fmt(n) {
            if (n == null) return '—';
            if (n >= 1e9) return (n / 1e9).toFixed(1) + ' млрд';
            if (n >= 1e6) return (n / 1e6).toFixed(1) + ' млн';
            if (n >= 1e3) return (n / 1e3).toFixed(1) + ' тыс';
            return String(n);
        }

        function collect() {
            var data = { views: null, likes: null, dislikes: null, date: '' };

            // Views + date from the description info line
            var info = document.querySelector('ytd-watch-info-text #info');
            if (info) {
                var parts = info.textContent.split('•').map(function (s) { return s.trim(); });
                for (var i = 0; i < parts.length; i++) {
                    if (/просмотр|views/i.test(parts[i])) data.views = parseCount(parts[i]);
                    else if (parts[i] && !data.date && !/просмотр|views/i.test(parts[i])) data.date = parts[i];
                }
            }

            // Likes from the like button label
            var likeBtn = document.querySelector(
                'like-button-view-model button, ytd-menu-renderer like-button-view-model button');
            if (likeBtn) {
                data.likes = parseCount(likeBtn.getAttribute('aria-label') || likeBtn.textContent);
            }

            // Dislikes from Return YouTube Dislike's rendered counter
            var disSelectors = [
                '#segmented-dislike-button .yt-spec-button-shape-next__button-text-content',
                '#segmented-dislike-button .ytSpecButtonShapeNextButtonTextContent',
                'dislike-button-view-model .yt-spec-button-shape-next__button-text-content',
                'dislike-button-view-model .ytSpecButtonShapeNextButtonTextContent',
                'dislike-button-view-model button span[role="text"]',
                'dislike-button-view-model button',
                '#segmented-dislike-button button',
                'ytd-toggle-button-renderer.ytd-segmented-like-dislike-button-renderer:last-of-type button'
            ];
            for (var si = 0; si < disSelectors.length; si++) {
                var dis = document.querySelector(disSelectors[si]);
                if (!dis) continue;
                var raw = (dis.getAttribute('aria-label') || dis.textContent || '').trim();
                var v = parseCount(raw);
                if (v != null) { data.dislikes = v; break; }
            }

            return data;
        }

        function render() {
            if (!/^\/watch/.test(location.pathname)) {
                var old = document.getElementById(PANEL_ID);
                if (old) old.remove();
                return;
            }
            var host = document.querySelector('#secondary #secondary-inner') ||
                       document.querySelector('#secondary');
            if (!host) return;

            ensureStyle();
            var d = collect();
            if (d.views == null && d.likes == null) return; // page not ready yet

            var ratioPct = null;
            if (d.likes != null && d.dislikes != null && d.likes + d.dislikes > 0) {
                ratioPct = Math.round(d.likes / (d.likes + d.dislikes) * 100);
            }
            var engagement = (d.likes != null && d.views) ?
                (d.likes / d.views * 100).toFixed(1) + '%' : '—';

            // Built with createElement/textContent only: YouTube enforces
            // Trusted Types, so any innerHTML assignment throws.
            function el(tag, cls, text) {
                var e = document.createElement(tag);
                if (cls) e.className = cls;
                if (text != null) e.textContent = text;
                return e;
            }
            function cell(label, value) {
                var c = el('div', 'lg-an-cell');
                c.appendChild(el('div', 'lg-an-label', label));
                c.appendChild(el('div', 'lg-an-value', value));
                return c;
            }

            var panel = document.getElementById(PANEL_ID);
            if (!panel) {
                panel = document.createElement('div');
                panel.id = PANEL_ID;
                host.prepend(panel);
            }
            while (panel.firstChild) panel.removeChild(panel.firstChild);

            panel.appendChild(el('div', 'lg-an-title', 'Аналитика видео'));
            var grid = el('div', 'lg-an-grid');
            grid.appendChild(cell('Просмотры', fmt(d.views)));
            grid.appendChild(cell('Вовлечённость', engagement));
            grid.appendChild(cell('Лайки', fmt(d.likes)));
            grid.appendChild(cell('Дизлайки', fmt(d.dislikes)));
            if (ratioPct != null) {
                var rc = el('div', 'lg-an-cell');
                rc.style.gridColumn = '1 / -1';
                rc.appendChild(el('div', 'lg-an-label', 'Рейтинг ' + ratioPct + '%'));
                var bar = el('div', 'lg-an-bar');
                var fill = el('div', 'lg-an-bar-fill');
                fill.style.width = ratioPct + '%';
                bar.appendChild(fill);
                rc.appendChild(bar);
                grid.appendChild(rc);
            }
            panel.appendChild(grid);
        }

        // Watch-page data loads asynchronously; poll a few times after each
        // navigation, then stay idle until the next one. Off watch pages the
        // first tick removes the panel and stops immediately instead of
        // burning through all 20 tries doing nothing.
        var timer = null;
        function stopTimer() {
            if (timer) { clearInterval(timer); timer = null; }
        }
        function schedule() {
            stopTimer();
            var tries = 0;
            timer = setInterval(function () {
                tries++;
                render();
                // RYD injects its dislike count noticeably later than YouTube
                // renders views/likes; keep polling a while longer if the
                // dislike cell is still empty so a late counter gets picked up.
                var doneEarly = tries > 20 && document.querySelector('#lg-analytics-panel');
                if (!/^\/watch/.test(location.pathname) || tries > 60 ||
                    (doneEarly && collect().dislikes != null)) stopTimer();
            }, 700);
        }
        // Drop the previous video's numbers as soon as navigation starts:
        // YouTube reuses the watch DOM, so the old card (and old like-button
        // labels) otherwise showed stale stats for the next video.
        document.addEventListener('yt-navigate-start', function () {
            stopTimer();
            var old = document.getElementById(PANEL_ID);
            if (old) old.remove();
        });
        document.addEventListener('yt-navigate-finish', schedule);
        document.addEventListener('DOMContentLoaded', schedule);
        schedule();
    })();
    "#
}
