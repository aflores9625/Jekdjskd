const test = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const script = fs.readFileSync(require('node:path').join(__dirname, '../src/ambient.js'), 'utf8');
function harness(options = {}) {
    const listeners = {}, windows = {}, intervals = [], elements = [], pending = new Map();
    let now = 1000, nextId = 0, draws = 0, animation = null;
    const rect = { left: 80, top: 120, right: 880, bottom: 570, width: 800, height: 450 };
    const video = { tagName: 'VIDEO', readyState: 4, videoWidth: 1920, videoHeight: 1080, currentTime: 1,
        currentSrc: 'video-a', paused: false, ended: false,
        getBoundingClientRect: () => rect,
        requestVideoFrameCallback: callback => { const id = nextId++; pending.set(id, callback); return id; },
        cancelVideoFrameCallback: id => pending.delete(id) };
    Object.assign(video, options.video);
    if (options.fallback) delete video.requestVideoFrameCallback;
    const body = { appendChild: element => { element.isConnected = true; } };
    const document = { body, hidden: false, fullscreenElement: null,
        addEventListener: (name, listener) => { (listeners[name] ||= []).push(listener); },
        querySelector: selector => selector.includes('video') ? document.video : selector === '#movie_player' ? { classList: { contains: () => false } } : selector.includes('#secondary') ? { getBoundingClientRect: () => ({ left: 904, top: 120, bottom: 900, width: 350 }) } : null,
        createElement: tag => {
            const element = { tagName: tag, style: {}, setAttribute() {}, appendChild() {},
                getContext: () => ({ clearRect() {}, drawImage() { draws++; },
                    getImageData() { if (options.blockRead) throw Error('SecurityError'); return { data: options.pixels || new Uint8ClampedArray(96 * 54 * 4).fill(100) }; } }) };
            elements.push(element); return element;
        }, video };
    const location = { pathname: '/watch' };
    const window = { addEventListener: (name, listener) => { windows[name] = listener; } };
    const timers = new Map();
    const context = { document, window, location, innerWidth: 1280, innerHeight: 900,
        matchMedia: () => ({ matches: false, addEventListener() {} }),
        performance: { now: () => now },
        setInterval: callback => intervals.push(callback),
        setTimeout: callback => { const id = nextId++; timers.set(id, callback); return id; },
        clearTimeout: id => timers.delete(id), requestAnimationFrame: callback => { animation = callback; return 1; } };
    vm.runInNewContext(script, context);
    return { document, video, location, pending, timers, rect,
        root: () => elements.find(element => element.id === 'lg-ambilight'),
        canvas: () => elements.find(element => element.tagName === 'canvas'),
        draws: () => draws, tick: () => { now += 1000; intervals.forEach(callback => callback()); },
        frame: () => { now += 40; video.currentTime += 0.1; const callbacks = [...pending.values()]; pending.clear(); callbacks.forEach(callback => callback()); },
        emit: name => (listeners[name] || []).forEach(listener => listener({ target: video })),
        scroll: () => { windows.scroll(); if (animation) { const callback = animation; animation = null; callback(); } } };
}
function framePixels(pattern) {
    const data = new Uint8ClampedArray(96 * 54 * 4);
    for (let row = 0; row < 54; row++) for (let col = 0; col < 96; col++) {
        const level = pattern(row, col);
        const offset = (row * 96 + col) * 4;
        data[offset] = data[offset + 1] = data[offset + 2] = level;
        data[offset + 3] = 255;
    }
    return data;
}
function nums(value) { return String(value).match(/-?\d+(?:\.\d+)?/g).map(Number); }
function near(actual, expected, tolerance = 0.01) { assert.ok(Math.abs(actual - expected) <= tolerance, actual + ' !~ ' + expected); }
function marginFor(w, h) { return Math.min(230, Math.max(48, Math.min(w, h) * 0.34)); }
test('automatic startup clips the sidebar and punches a video-shaped hole', () => {
    const app = harness();
    assert.ok(Number(app.root().style.opacity) > 0);
    assert.equal(app.root().style.clipPath, 'inset(0px 386px 0px 0px)');
    assert.match(app.canvas().style.clipPath, /^polygon\(evenodd/);
    assert.equal(app.pending.size, 1);
});
test('hidden document cancels callback including callback ID zero', () => {
    const app = harness(); app.document.hidden = true; app.emit('visibilitychange');
    assert.equal(app.pending.size, 0); assert.equal(app.root().style.opacity, '0');
    app.document.hidden = false; app.emit('visibilitychange'); assert.equal(app.pending.size, 1);
});
test('paused playback does not repeatedly sample video', () => {
    const app = harness(); app.video.paused = true; app.emit('pause');
    const draws = app.draws(); app.tick(); app.tick();
    assert.equal(app.draws(), draws); assert.equal(app.pending.size, 0);
});
test('navigation and fullscreen remove light', () => {
    const app = harness(); app.location.pathname = '/'; app.emit('yt-navigate-finish');
    assert.equal(app.root().style.opacity, '0'); assert.equal(app.pending.size, 0);
    app.location.pathname = '/watch'; app.document.fullscreenElement = {}; app.emit('fullscreenchange');
    assert.equal(app.root().style.opacity, '0');
});
test('video replacement cancels old callback and resumes sampling', () => {
    const app = harness(); app.document.video = { ...app.video, currentSrc: 'video-b' }; app.tick();
    assert.equal(app.pending.size, 1); assert.ok(Number(app.root().style.opacity) > 0);
});
test('blocked pixel read does not disable drawable ambient light', () => {
    const app = harness({ blockRead: true });
    assert.ok(Number(app.root().style.opacity) > 0); assert.equal(app.pending.size, 1);
});
test('fallback timer is cancelled when page is hidden', () => {
    const app = harness({ fallback: true }); assert.equal(app.timers.size, 1);
    app.document.hidden = true; app.emit('visibilitychange'); assert.equal(app.timers.size, 0);
});
test('paused glow returns after scrolling back into view', () => {
    const app = harness(); app.video.paused = true; app.emit('pause');
    app.rect.top = -600; app.rect.bottom = -150; app.scroll();
    assert.equal(app.root().style.opacity, '0');
    app.rect.top = 120; app.rect.bottom = 570; app.scroll();
    assert.ok(Number(app.root().style.opacity) > 0);
});
test('letterboxed video places canvas and hole over the content rect', () => {
    const app = harness({ pixels: framePixels((row) => (row < 7 || row > 46) ? 0 : 100) });
    app.frame();
    const contentWidth = 800, contentHeight = 40 / 54 * 450, contentTop = 120 + 7 / 54 * 450;
    const margin = marginFor(contentWidth, contentHeight);
    near(parseFloat(app.canvas().style.left), 80 - margin);
    near(parseFloat(app.canvas().style.top), contentTop - margin);
    near(parseFloat(app.canvas().style.width), contentWidth + margin * 2);
    near(parseFloat(app.canvas().style.height), contentHeight + margin * 2);
    const hole = nums(app.canvas().style.clipPath).slice(10);
    near(hole[0], margin - 1); near(hole[1], margin - 1);
    near(hole[4], margin + contentWidth + 1); near(hole[5], margin + contentHeight + 1);
    assert.ok(contentHeight < 450 - 1, 'content rect must be shorter than the element rect');
    assert.ok(Number(app.root().style.opacity) > 0);
});
test('pillarbox bars wider than the old 19-column cap are detected', () => {
    const app = harness({ video: { videoWidth: 1080, videoHeight: 1920 },
        pixels: framePixels((row, col) => (col < 34 || col > 61) ? 0 : 100) });
    app.frame();
    const contentLeft = 80 + 34 / 96 * 800, contentWidth = 28 / 96 * 800;
    const margin = marginFor(contentWidth, 450);
    near(parseFloat(app.canvas().style.left), contentLeft - margin);
    near(parseFloat(app.canvas().style.width), contentWidth + margin * 2);
    near(parseFloat(app.canvas().style.top), 120 - margin);
    assert.ok(contentLeft - 80 > 19 / 96 * 800, 'detected bar must exceed the legacy 19-column scan cap');
});
test('glow canvas carries no corner-killing radial mask', () => {
    const app = harness();
    assert.ok(!/radial/.test(app.canvas().style.cssText || ''));
    assert.equal(app.canvas().style.maskImage, undefined);
    assert.match(app.root().style.maskImage, /^linear-gradient/);
});
test('asymmetric dark edge is not treated as a bar', () => {
    const app = harness({ pixels: framePixels((row, col) => (col < 20 ? 0 : 100)) });
    app.frame();
    const margin = marginFor(800, 450);
    near(parseFloat(app.canvas().style.left), 80 - margin);
    near(parseFloat(app.canvas().style.top), 120 - margin);
    near(parseFloat(app.canvas().style.width), 800 + margin * 2);
    near(parseFloat(app.canvas().style.height), 450 + margin * 2);
});
