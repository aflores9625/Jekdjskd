// Startup screen: a red dot pops, bursts into bubbles and grows into the
// YouTube play logo, which then breathes with floating bubbles until the app
// shell has rendered. Only transform/opacity are animated so everything runs
// on the compositor and stays smooth while YouTube keeps the main thread busy.
// Built with createElement - Trusted Types on youtube.com forbid innerHTML.
(function () {
    'use strict';
    if (window.__ygSplash) return;
    window.__ygSplash = true;

    var MIN_VISIBLE_MS = 1300; // let the intro finish
    var MAX_VISIBLE_MS = 15000;
    var started = Date.now();
    var root = null;
    var done = false;

    var COLORS = ['#ff0033', '#ff3355', '#ff6b81', '#ff99a8', '#ffffff'];
    var BURST = 14;
    var FLOAT = 12;

    function rnd(a, b) { return a + Math.random() * (b - a); }
    function px(n) { return n.toFixed(1) + 'px'; }

    var CSS =
        '#yg-splash{position:fixed;inset:0;z-index:2147483647;display:flex;' +
            'align-items:center;justify-content:center;background:#0d0d0d;contain:strict;' +
            'transition:opacity .5s cubic-bezier(.22,1,.36,1),visibility 0s linear .5s;}' +
        '#yg-splash.yg-out{opacity:0;visibility:hidden;pointer-events:none;}' +
        '#yg-splash .yg-stage{position:relative;width:240px;height:240px;' +
            'transition:transform .5s cubic-bezier(.4,0,1,1);}' +
        '#yg-splash.yg-out .yg-stage{transform:scale(1.12);}' +
        '#yg-splash .yg-c{position:absolute;left:50%;top:50%;will-change:transform,opacity;}' +
        '#yg-splash .yg-glow{width:260px;height:190px;margin:-95px 0 0 -130px;' +
            'background:radial-gradient(closest-side,rgba(255,0,51,.32),rgba(255,0,51,0));opacity:0;' +
            'animation:yg-glow-in .8s ease-out .4s forwards,yg-glow 2.4s ease-in-out 1.3s infinite;}' +
        '#yg-splash .yg-dot{width:18px;height:18px;margin:-9px 0 0 -9px;border-radius:50%;' +
            'background:#ff0033;opacity:0;animation:yg-dot .6s cubic-bezier(.3,.7,.4,1) both;}' +
        '#yg-splash .yg-ring{width:40px;height:40px;margin:-20px 0 0 -20px;border-radius:50%;' +
            'border:2px solid #ff3355;opacity:0;box-sizing:border-box;' +
            'animation:yg-ring .7s cubic-bezier(.2,.8,.3,1) .34s both;}' +
        '#yg-splash .yg-b,#yg-splash .yg-f{border-radius:50%;opacity:0;}' +
        '#yg-splash .yg-logo{width:112px;height:80px;margin:-40px 0 0 -56px;border-radius:24px;' +
            'overflow:hidden;background:linear-gradient(160deg,#ff3a5c 0%,#ff0033 45%,#e0002d 100%);' +
            'box-shadow:0 12px 34px rgba(255,0,51,.33);opacity:0;' +
            'animation:yg-grow .8s cubic-bezier(.3,.7,.4,1) .38s both,' +
                'yg-breathe 2.4s ease-in-out 1.3s infinite;}' +
        '#yg-splash .yg-tri{left:50%;top:50%;width:30px;height:34px;margin:-17px 0 0 -12px;' +
            'background:#fff;clip-path:polygon(0 0,100% 50%,0 100%);opacity:0;' +
            'animation:yg-tri .55s cubic-bezier(.3,1.6,.5,1) .85s both,' +
                'yg-nudge 2.4s ease-in-out 1.4s infinite;}' +
        '#yg-splash .yg-shine{left:0;top:-20px;width:34px;height:120px;' +
            'background:linear-gradient(90deg,rgba(255,255,255,0),rgba(255,255,255,.32),rgba(255,255,255,0));' +
            'transform:translateX(-80px) skewX(-20deg);' +
            'animation:yg-shine 2.4s ease-in-out 1.1s infinite;}' +
        '@keyframes yg-dot{0%{transform:scale(0);opacity:1}35%{transform:scale(1.3);opacity:1}' +
            '55%{transform:scale(.9)}65%{transform:scale(1);opacity:1}' +
            '100%{transform:scale(3);opacity:0}}' +
        '@keyframes yg-ring{0%{transform:scale(.3);opacity:.9}100%{transform:scale(4.2);opacity:0}}' +
        '@keyframes yg-grow{0%{transform:scale(.14,.2);opacity:0}' +
            '15%{opacity:1}55%{transform:scale(1.1,.9)}' +
            '78%{transform:scale(.96,1.05)}100%{transform:scale(1);opacity:1}}' +
        '@keyframes yg-breathe{0%,100%{transform:scale(1)}50%{transform:scale(1.045)}}' +
        '@keyframes yg-tri{0%{transform:scale(0) rotate(-90deg);opacity:0}' +
            '100%{transform:none;opacity:1}}' +
        '@keyframes yg-nudge{0%,100%{transform:none}50%{transform:translateX(2px) scale(1.06)}}' +
        '@keyframes yg-shine{0%,50%{transform:translateX(-80px) skewX(-20deg)}' +
            '85%,100%{transform:translateX(160px) skewX(-20deg)}}' +
        '@keyframes yg-glow-in{to{opacity:1}}' +
        '@keyframes yg-glow{0%,100%{transform:scale(1)}50%{transform:scale(1.12)}}';

    // Each bubble gets its own keyframes; custom properties inside keyframes
    // would keep the animations off the compositor.
    function bubble(parent, cls, i, css) {
        var size = cls === 'yg-b' ? rnd(6, 16) : rnd(5, 12);
        var el = document.createElement('div');
        el.className = 'yg-c ' + cls;
        el.style.width = el.style.height = px(size);
        el.style.margin = px(-size / 2) + ' 0 0 ' + px(-size / 2);
        el.style.background = COLORS[Math.floor(Math.random() * COLORS.length)];
        var name = cls + i;
        if (cls === 'yg-b') {
            var a = (i / BURST) * Math.PI * 2 + rnd(-0.2, 0.2);
            var d = rnd(72, 112);
            var x = Math.cos(a) * d, y = Math.sin(a) * d * 0.8;
            css.push('@keyframes ' + name + '{0%{transform:translate(0,0) scale(0);opacity:1}' +
                '30%{transform:translate(' + px(x * 0.6) + ',' + px(y * 0.6) + ') scale(1);opacity:1}' +
                '100%{transform:translate(' + px(x) + ',' + px(y) + ') scale(0);opacity:0}}');
            el.style.animation = name + ' ' + rnd(0.75, 1.05).toFixed(2) +
                's cubic-bezier(.2,.8,.3,1) ' + rnd(0.34, 0.5).toFixed(2) + 's both';
        } else {
            // Rise along the logo's sides so the body doesn't hide them.
            var side = i % 2 ? 1 : -1;
            var sx = side * rnd(60, 95), sy = rnd(-5, 40), rise = rnd(50, 90), drift = side * rnd(0, 18);
            css.push('@keyframes ' + name + '{0%{transform:translate(' + px(sx) + ',' + px(sy) +
                ') scale(0);opacity:0}20%{opacity:.85}' +
                '60%{transform:translate(' + px(sx + drift * 0.6) + ',' + px(sy - rise * 0.6) + ') scale(1)}' +
                '100%{transform:translate(' + px(sx + drift) + ',' + px(sy - rise) + ') scale(.3);opacity:0}}');
            el.style.animation = name + ' ' + rnd(2.2, 3.4).toFixed(2) + 's ease-out ' +
                rnd(1.0, 3.0).toFixed(2) + 's infinite';
        }
        parent.appendChild(el);
    }

    function div(parent, cls) {
        var el = document.createElement('div');
        el.className = cls;
        parent.appendChild(el);
        return el;
    }

    function build() {
        var host = document.documentElement;
        if (root || done || !host) return;
        var css = [CSS];

        root = document.createElement('div');
        root.id = 'yg-splash';
        root.setAttribute('role', 'status');
        root.setAttribute('aria-label', 'Загрузка YouTube');
        var style = document.createElement('style');
        root.appendChild(style);

        var stage = div(root, 'yg-stage');
        div(stage, 'yg-c yg-glow');
        var i;
        for (i = 0; i < FLOAT; i++) bubble(stage, 'yg-f', i, css);
        div(stage, 'yg-c yg-ring');
        for (i = 0; i < BURST; i++) bubble(stage, 'yg-b', i, css);
        div(stage, 'yg-c yg-dot');
        var logo = div(stage, 'yg-c yg-logo');
        div(logo, 'yg-c yg-shine');
        div(logo, 'yg-c yg-tri');

        css.push('@media (prefers-reduced-motion:reduce){#yg-splash *{animation:none!important}' +
            '#yg-splash .yg-logo,#yg-splash .yg-tri,#yg-splash .yg-glow{opacity:1}}');
        style.textContent = css.join('');
        host.appendChild(root);
    }

    function hide() {
        if (done) return;
        var wait = MIN_VISIBLE_MS - (Date.now() - started);
        if (wait > 0) { setTimeout(hide, wait); return; }
        done = true;
        if (!root) return;
        root.classList.add('yg-out');
        setTimeout(function () { if (root) root.remove(); root = null; }, 550);
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
