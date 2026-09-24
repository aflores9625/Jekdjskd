(function () {
    'use strict';
    if (window.__lgAmbilight) return;
    window.__lgAmbilight = true;
    const sampleWidth = 96, sampleHeight = 54;
    const width = 256, height = 144;
    const motion = matchMedia('(prefers-reduced-motion: reduce)');
    let root, canvas, context, sample, sampleContext, raw, rawContext;
    let video = null, callback = null, fallback = null;
    let lastFrame = 0, lastMedia = -1, lastAnalysis = 0, source = '';
    let readBlocked = false, failed = false, bounds = [0, 0, sampleWidth, sampleHeight];
    let crop = [0, 0, sampleWidth, sampleHeight];
    let luminance = 0.35, geometry = '', layoutFrame = null;
    function eligible() {
        return location.pathname === '/watch' && !document.hidden && !document.fullscreenElement;
    }
    let shownOpacity = '';
    // Skip redundant writes: every write restarts the opacity transition.
    function setOpacity(value) {
        const next = value === 0 ? '0' : value.toFixed(2);
        if (!root || next === shownOpacity) return;
        shownOpacity = next;
        root.style.opacity = next;
    }
    function hide() { setOpacity(0); }
    function cancel() {
        if (callback !== null && video && video.cancelVideoFrameCallback) video.cancelVideoFrameCallback(callback);
        callback = null;
        if (fallback !== null) clearTimeout(fallback);
        fallback = null;
    }
    function reset() {
        lastFrame = 0; lastMedia = -1; lastAnalysis = 0; readBlocked = false; failed = false;
        luminance = 0.35; bounds = [0, 0, sampleWidth, sampleHeight]; crop = bounds.slice();
        if (sample) sample.width = sampleWidth;
        if (raw) raw.width = sampleWidth;
    }
    function ensureDom() {
        if (!document.body) return false;
        if (!root) {
            root = document.createElement('div');
            root.id = 'lg-ambilight';
            root.setAttribute('aria-hidden', 'true');
            root.style.cssText = 'position:fixed;inset:0;pointer-events:none;z-index:2200;opacity:0;transition:opacity .65s ease;mix-blend-mode:screen;contain:strict;';
            canvas = document.createElement('canvas');
            canvas.width = width; canvas.height = height;
            canvas.style.cssText = 'position:absolute;pointer-events:none;';
            context = canvas.getContext('2d');
            sample = document.createElement('canvas');
            sample.width = sampleWidth; sample.height = sampleHeight;
            sampleContext = sample.getContext('2d');
            raw = document.createElement('canvas');
            raw.width = sampleWidth; raw.height = sampleHeight;
            rawContext = raw.getContext('2d', { willReadFrequently: true });
            root.appendChild(canvas);
        }
        if (!root.isConnected) document.body.appendChild(root);
        return !!(context && sampleContext && rawContext);
    }
    function predictedBounds(rect) {
        if (!video || !(video.videoWidth > 0) || !(video.videoHeight > 0) || !(rect.width > 0) || !(rect.height > 0)) return null;
        const videoAspect = video.videoWidth / video.videoHeight;
        const elementAspect = rect.width / rect.height;
        if (Math.abs(videoAspect - elementAspect) <= elementAspect * 0.02) return null;
        if (videoAspect > elementAspect) {
            const bar = (1 - elementAspect / videoAspect) / 2 * sampleHeight;
            if (bar < 1.5 || bar > sampleHeight * 0.45) return null;
            return [0, bar, sampleWidth, sampleHeight - bar * 2];
        }
        const bar = (1 - videoAspect / elementAspect) / 2 * sampleWidth;
        if (bar < 1.5 || bar > sampleWidth * 0.45) return null;
        return [bar, 0, sampleWidth - bar * 2, sampleHeight];
    }
    function effectiveBounds(rect) {
        const predicted = predictedBounds(rect);
        if (!predicted) return bounds;
        const left = Math.max(bounds[0], predicted[0]);
        const top = Math.max(bounds[1], predicted[1]);
        const right = Math.min(bounds[0] + bounds[2], predicted[0] + predicted[2]);
        const bottom = Math.min(bounds[1] + bounds[3], predicted[1] + predicted[3]);
        if (right - left < 8 || bottom - top < 8) return predicted;
        return [left, top, right - left, bottom - top];
    }
    function contentRect(rect, box) {
        return {
            left: rect.left + box[0] / sampleWidth * rect.width,
            top: rect.top + box[1] / sampleHeight * rect.height,
            width: box[2] / sampleWidth * rect.width,
            height: box[3] / sampleHeight * rect.height
        };
    }
    function layout() {
        if (!video || !root) return null;
        const element = video.getBoundingClientRect();
        if (element.width < 160 || element.height < 90 || element.bottom <= 0 || element.top >= innerHeight || element.right <= 0 || element.left >= innerWidth) return null;
        const player = document.querySelector('#movie_player');
        if (player && (player.classList.contains('ytp-player-minimized') || player.classList.contains('ytp-player-minimized-by-user'))) return null;
        crop = effectiveBounds(element);
        const rect = contentRect(element, crop);
        if (rect.width < 24 || rect.height < 24) return null;
        const margin = Math.min(230, Math.max(48, Math.min(rect.width, rect.height) * 0.34));
        let right = innerWidth, top = 0;
        const secondary = document.querySelector('ytd-watch-flexy #secondary');
        if (secondary) {
            const side = secondary.getBoundingClientRect();
            if (side.width > 0 && side.left >= rect.left + rect.width - 8 && side.top < rect.top + rect.height + margin && side.bottom > rect.top - margin) right = Math.min(right, side.left - 10);
        }
        const masthead = document.querySelector('#masthead-container');
        if (masthead) {
            const head = masthead.getBoundingClientRect();
            if (head.bottom > 0 && head.bottom <= rect.top + 8) top = head.bottom;
        }
        const canvasWidth = rect.width + margin * 2, canvasHeight = rect.height + margin * 2;
        const key = [rect.left, rect.top, rect.width, rect.height, right, top, innerWidth, innerHeight].join(':');
        if (key !== geometry) {
            geometry = key;
            const leftHole = margin - 1, rightHole = margin + rect.width + 1;
            const topHole = margin - 1, bottomHole = margin + rect.height + 1;
            root.style.clipPath = 'inset(' + top + 'px ' + Math.max(0, innerWidth - right) + 'px 0px 0px)';
            root.style.maskImage = 'linear-gradient(to right,#000 ' + Math.max(0, right - 36) + 'px,transparent ' + right + 'px)';
            canvas.style.clipPath = 'polygon(evenodd,0% 0%,100% 0%,100% 100%,0% 100%,0% 0%,' + leftHole + 'px ' + topHole + 'px,' + leftHole + 'px ' + bottomHole + 'px,' + rightHole + 'px ' + bottomHole + 'px,' + rightHole + 'px ' + topHole + 'px,' + leftHole + 'px ' + topHole + 'px)';
            Object.assign(canvas.style, {
                left: (rect.left - margin) + 'px', top: (rect.top - margin) + 'px',
                width: canvasWidth + 'px', height: canvasHeight + 'px',
                filter: 'blur(' + Math.min(58, margin * 0.26) + 'px) saturate(1.28)'
            });
        }
        return { edgeX: width * margin / canvasWidth, edgeY: height * margin / canvasHeight };
    }
    function analyze(now) {
        if (readBlocked || now - lastAnalysis < 500) return;
        lastAnalysis = now;
        let pixels;
        // CPU readback only here (2x/s); per-frame drawing stays on the GPU.
        rawContext.drawImage(video, 0, 0, sampleWidth, sampleHeight);
        try { pixels = rawContext.getImageData(0, 0, sampleWidth, sampleHeight).data; }
        catch (_) { readBlocked = true; return; }
        const maxTop = Math.floor(sampleHeight * 0.45), maxLeft = Math.floor(sampleWidth * 0.45);
        function bright(horizontal, position) {
            const count = horizontal ? sampleWidth : sampleHeight;
            let lit = 0;
            for (let index = 0; index < count; index++) {
                const offset = (horizontal ? position * sampleWidth + index : index * sampleWidth + position) * 4;
                if (Math.max(pixels[offset], pixels[offset + 1], pixels[offset + 2]) > 18) lit++;
            }
            return lit > count * 0.08;
        }
        let top = 0, bottom = sampleHeight - 1, left = 0, right = sampleWidth - 1;
        while (top < maxTop && !bright(true, top)) top++;
        while (bottom > sampleHeight - 1 - maxTop && !bright(true, bottom)) bottom--;
        while (left < maxLeft && !bright(false, left)) left++;
        while (right > sampleWidth - 1 - maxLeft && !bright(false, right)) right--;
        const verticalBars = top > 0 && top < maxTop && Math.abs(top - (sampleHeight - 1 - bottom)) <= 2;
        const horizontalBars = left > 0 && left < maxLeft && Math.abs(left - (sampleWidth - 1 - right)) <= 2;
        bounds = [horizontalBars ? left : 0, verticalBars ? top : 0,
            horizontalBars ? right - left + 1 : sampleWidth, verticalBars ? bottom - top + 1 : sampleHeight];
        let total = 0;
        for (let offset = 0; offset < pixels.length; offset += 4) total += pixels[offset] * 0.2126 + pixels[offset + 1] * 0.7152 + pixels[offset + 2] * 0.0722;
        const next = total / (sampleWidth * sampleHeight * 255);
        luminance += (next - luminance) * 0.25;
    }
    function schedule() {
        if (!eligible() || !video || video.paused || video.ended || failed || callback !== null || fallback !== null) return;
        if (video.requestVideoFrameCallback) {
            callback = video.requestVideoFrameCallback(function () { callback = null; render(false); });
        } else {
            fallback = setTimeout(function () { fallback = null; render(false); }, motion.matches ? 125 : 34);
        }
    }
    function render(force) {
        if (!eligible() || !video || video.readyState < 2 || !video.videoWidth || !ensureDom()) { hide(); return; }
        if (source !== video.currentSrc) { source = video.currentSrc; reset(); }
        if (failed) { hide(); return; }
        const metrics = layout();
        if (!metrics) { hide(); return; }
        const now = performance.now();
        if (lastFrame) setOpacity((0.58 + Math.sqrt(Math.max(0, luminance)) * 0.24) * (video.paused || video.ended ? 0.7 : 1));
        if (!force && (now - lastFrame < (motion.matches ? 125 : 32) || video.currentTime === lastMedia)) { schedule(); return; }
        try {
            analyze(now);
            const scaleX = video.videoWidth / sampleWidth, scaleY = video.videoHeight / sampleHeight;
            sampleContext.globalAlpha = lastFrame ? 1 - Math.exp(-Math.max(1, now - lastFrame) / (motion.matches ? 450 : 140)) : 1;
            sampleContext.drawImage(video, crop[0] * scaleX, crop[1] * scaleY, crop[2] * scaleX, crop[3] * scaleY, 0, 0, sampleWidth, sampleHeight);
            const edgeX = metrics.edgeX, edgeY = metrics.edgeY;
            const middleWidth = width - 2 * edgeX, middleHeight = height - 2 * edgeY;
            context.clearRect(0, 0, width, height);
            context.drawImage(sample, 0, 0, 4, sampleHeight, 0, edgeY, edgeX, middleHeight);
            context.drawImage(sample, sampleWidth - 4, 0, 4, sampleHeight, width - edgeX, edgeY, edgeX, middleHeight);
            context.drawImage(sample, 0, 0, sampleWidth, 3, edgeX, 0, middleWidth, edgeY);
            context.drawImage(sample, 0, sampleHeight - 3, sampleWidth, 3, edgeX, height - edgeY, middleWidth, edgeY);
            [[0, 0, 0, 0], [sampleWidth - 4, 0, width - edgeX, 0], [0, sampleHeight - 3, 0, height - edgeY], [sampleWidth - 4, sampleHeight - 3, width - edgeX, height - edgeY]].forEach(function (corner) {
                context.drawImage(sample, corner[0], corner[1], 4, 3, corner[2], corner[3], edgeX, edgeY);
            });
            lastFrame = now; lastMedia = video.currentTime;
            setOpacity((0.58 + Math.sqrt(Math.max(0, luminance)) * 0.24) * (video.paused || video.ended ? 0.7 : 1));
        } catch (_) { failed = true; hide(); return; }
        schedule();
    }
    function refresh() {
        cancel();
        const next = document.querySelector('#movie_player video') || document.querySelector('video');
        if (next !== video) { video = next; source = ''; reset(); }
        render(true);
    }
    function requestLayout() {
        if (layoutFrame !== null) return;
        layoutFrame = requestAnimationFrame(function () { layoutFrame = null; render(false); });
    }
    ['yt-navigate-finish','DOMContentLoaded','fullscreenchange','visibilitychange'].forEach(function (name) {
        document.addEventListener(name, refresh);
    });
    document.addEventListener('yt-navigate-start', function () { cancel(); hide(); });
    ['play','pause','seeked','loadeddata','emptied','ended'].forEach(function (name) {
        document.addEventListener(name, function (event) { if (event.target.tagName === 'VIDEO') refresh(); }, true);
    });
    window.addEventListener('resize', requestLayout);
    window.addEventListener('scroll', requestLayout, { capture: true, passive: true });
    window.addEventListener('pagehide', function () { cancel(); hide(); });
    window.addEventListener('pageshow', refresh);
    motion.addEventListener('change', refresh);
    setInterval(function () {
        if (!eligible()) { cancel(); hide(); return; }
        const current = document.querySelector('#movie_player video') || document.querySelector('video');
        if (current !== video || (video && (!root || !root.isConnected || source !== video.currentSrc))) refresh();
        else if (video) {
            if (!layout()) hide();
            else if (callback === null && fallback === null && !video.paused && !video.ended) render(false);
        } else hide();
    }, 1000);
    refresh();
})();
