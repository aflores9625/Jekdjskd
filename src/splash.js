// Startup screen: an animated YouTube logo over the page until the app shell
// has rendered. Built with createElement/createElementNS - Trusted Types on
// youtube.com forbid innerHTML.
(function () {
    'use strict';
    if (window.__ygSplash) return;
    window.__ygSplash = true;

    var SVG_NS = 'http://www.w3.org/2000/svg';
    var MIN_VISIBLE_MS = 700;
    var MAX_VISIBLE_MS = 15000;
    var started = Date.now();
    var root = null;
    var done = false;

    var CSS =
        '#yg-splash{position:fixed;inset:0;z-index:2147483647;display:flex;' +
            'align-items:center;justify-content:center;background:#0d0d0d;' +
            'transition:opacity .45s cubic-bezier(.22,1,.36,1),visibility 0s linear .45s;}' +
        '#yg-splash.yg-out{opacity:0;visibility:hidden;pointer-events:none;}' +
        '#yg-splash .yg-stage{position:relative;width:112px;height:79px;display:flex;' +
            'align-items:center;justify-content:center;}' +
        '#yg-splash .yg-ring{position:absolute;inset:0;border-radius:24px;' +
            'border:2px solid rgba(255,0,51,.6);opacity:0;' +
            'animation:yg-ring 2.1s cubic-bezier(.22,1,.36,1) .7s infinite;}' +
        '#yg-splash .yg-ring.yg-late{animation-delay:1.05s;}' +
        '#yg-splash svg{position:relative;width:112px;height:79px;overflow:visible;' +
            'animation:yg-pop .7s cubic-bezier(.34,1.56,.64,1) both,' +
            'yg-beat 2.1s ease-in-out .7s infinite;}' +
        '#yg-splash .yg-play{transform-box:fill-box;transform-origin:center;' +
            'animation:yg-play-in .5s cubic-bezier(.34,1.56,.64,1) .3s both,' +
            'yg-play-nudge 2.1s ease-in-out 1s infinite;}' +
        '#yg-splash .yg-shine{animation:yg-shine 2.1s ease-in-out .7s infinite;}' +
        '#yg-splash.yg-out svg{animation:yg-exit .45s cubic-bezier(.4,0,1,1) forwards;}' +
        '@keyframes yg-pop{0%{transform:scale(.4);opacity:0}100%{transform:scale(1);opacity:1}}' +
        '@keyframes yg-beat{0%,100%{transform:scale(1);filter:drop-shadow(0 0 6px rgba(255,0,51,.25))}' +
            '14%{transform:scale(1.08);filter:drop-shadow(0 0 22px rgba(255,0,51,.6))}' +
            '28%{transform:scale(.98)}42%{transform:scale(1.04)}60%{transform:scale(1)}}' +
        '@keyframes yg-play-in{0%{transform:translateX(-10px) scale(0);opacity:0}' +
            '100%{transform:none;opacity:1}}' +
        '@keyframes yg-play-nudge{0%,100%{transform:none}14%{transform:translateX(2px) scale(1.08)}}' +
        '@keyframes yg-ring{0%{transform:scale(1);opacity:.9}100%{transform:scale(1.7);opacity:0}}' +
        '@keyframes yg-shine{0%,40%{transform:translateX(-60px)}75%,100%{transform:translateX(130px)}}' +
        '@keyframes yg-exit{to{transform:scale(1.35);opacity:0}}' +
        '@media (prefers-reduced-motion:reduce){#yg-splash *{animation:none!important}}';

    function svgEl(tag, attrs) {
        var el = document.createElementNS(SVG_NS, tag);
        for (var k in attrs) el.setAttribute(k, attrs[k]);
        return el;
    }

    function build() {
        var host = document.documentElement;
        if (root || done || !host) return;
        var style = document.createElement('style');
        style.textContent = CSS;

        root = document.createElement('div');
        root.id = 'yg-splash';
        root.setAttribute('role', 'progressbar');
        root.setAttribute('aria-label', 'Загрузка YouTube');
        root.appendChild(style);

        var stage = document.createElement('div');
        stage.className = 'yg-stage';
        var ring = document.createElement('div');
        ring.className = 'yg-ring';
        var ring2 = document.createElement('div');
        ring2.className = 'yg-ring yg-late';

        // YouTube play-button mark: rounded red body, white triangle, and a
        // light sweep clipped to the body.
        var svg = svgEl('svg', { viewBox: '0 0 100 70', 'aria-hidden': 'true' });
        var body = 'M97.9 10.9A12.5 12.5 0 0 0 89.1 2.1C81.3 0 50 0 50 0S18.7 0 10.9 2.1' +
            'A12.5 12.5 0 0 0 2.1 10.9C0 18.7 0 35 0 35s0 16.3 2.1 24.1a12.5 12.5 0 0 0 8.8 8.8' +
            'C18.7 70 50 70 50 70s31.3 0 39.1-2.1a12.5 12.5 0 0 0 8.8-8.8C100 51.3 100 35 100 35' +
            's0-16.3-2.1-24.1z';
        var defs = svgEl('defs', {});
        var clip = svgEl('clipPath', { id: 'yg-splash-clip' });
        clip.appendChild(svgEl('path', { d: body }));
        var grad = svgEl('linearGradient', { id: 'yg-splash-shine', x1: '0', x2: '1', y1: '0', y2: '0' });
        grad.appendChild(svgEl('stop', { offset: '0', 'stop-color': '#fff', 'stop-opacity': '0' }));
        grad.appendChild(svgEl('stop', { offset: '.5', 'stop-color': '#fff', 'stop-opacity': '.35' }));
        grad.appendChild(svgEl('stop', { offset: '1', 'stop-color': '#fff', 'stop-opacity': '0' }));
        defs.appendChild(clip);
        defs.appendChild(grad);
        svg.appendChild(defs);
        svg.appendChild(svgEl('path', { d: body, fill: '#ff0033' }));
        var shine = svgEl('g', { 'clip-path': 'url(#yg-splash-clip)' });
        var bar = svgEl('g', { class: 'yg-shine' });
        bar.appendChild(svgEl('rect', {
            x: '0', y: '-10', width: '30', height: '90',
            fill: 'url(#yg-splash-shine)', transform: 'skewX(-20)',
        }));
        shine.appendChild(bar);
        svg.appendChild(shine);
        svg.appendChild(svgEl('path', { class: 'yg-play', d: 'M40 50 66 35 40 20z', fill: '#fff' }));

        stage.appendChild(ring);
        stage.appendChild(ring2);
        stage.appendChild(svg);
        root.appendChild(stage);
        host.appendChild(root);
    }

    function hide() {
        if (done) return;
        var wait = MIN_VISIBLE_MS - (Date.now() - started);
        if (wait > 0) { setTimeout(hide, wait); return; }
        done = true;
        if (!root) return;
        root.classList.add('yg-out');
        setTimeout(function () { if (root) root.remove(); root = null; }, 500);
    }

    // At document-creation time <html> may not exist yet.
    build();
    if (!root) {
        var mo = new MutationObserver(function () {
            build();
            if (root || done) mo.disconnect();
        });
        mo.observe(document, { childList: true });
    }

    // YouTube fires yt-navigate-finish once the first page has rendered.
    document.addEventListener('yt-navigate-finish', hide, { once: true });
    window.addEventListener('load', function () { setTimeout(hide, 2500); }, { once: true });
    setTimeout(hide, MAX_VISIBLE_MS);
})();
