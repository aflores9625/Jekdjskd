(function () {
    'use strict';
    if (window.__ygPlayerExtras) return;
    window.__ygPlayerExtras = true;
    let trackedVideo = null, subscriptions = null, queued = false, toastTimer = null;
    function currentVideo() { return document.querySelector('#movie_player video'); }
    function notify(message) {
        if (!document.body) return;
        let toast = document.getElementById('yg-player-notice');
        if (!toast) {
            toast = document.createElement('div');
            toast.id = 'yg-player-notice';
            toast.setAttribute('role', 'status');
            toast.style.cssText = 'position:fixed;bottom:84px;left:50%;transform:translateX(-50%);max-width:80vw;padding:12px 20px;border-radius:14px;background:rgba(18,22,30,.95);color:#f4f7fc;border:1px solid #ffffff25;box-shadow:0 8px 32px #0006;font:14px Segoe UI,sans-serif;pointer-events:none;z-index:99999;text-align:center;';
        }
        const host = document.fullscreenElement || document.body;
        if (toast.parentElement !== host) host.appendChild(toast);
        toast.textContent = message;
        toast.hidden = false;
        clearTimeout(toastTimer);
        toastTimer = setTimeout(function () { toast.hidden = true; }, 2400);
    }
    async function togglePip() {
        const video = currentVideo();
        if (!video || video.readyState < 2) return;
        try {
            if (document.pictureInPictureElement) await document.exitPictureInPicture();
            else if (document.pictureInPictureEnabled && !video.disablePictureInPicture) await video.requestPictureInPicture();
            else notify('Картинка в картинке недоступна для этого видео');
        } catch (_) { notify('Не удалось открыть картинку в картинке'); }
    }
    function syncControls() {
        const video = currentVideo();
        if (video !== trackedVideo) {
            if (subscriptions) subscriptions.abort();
            subscriptions = new AbortController();
            trackedVideo = video;
            if (video) ['enterpictureinpicture', 'leavepictureinpicture', 'loadedmetadata'].forEach(function (name) {
                video.addEventListener(name, syncControls, { signal: subscriptions.signal });
            });
        }
        const controls = document.querySelector('#movie_player .ytp-right-controls');
        if (!controls || !video || !document.pictureInPictureEnabled) return;
        let button = controls.querySelector('.yg-pip-button');
        if (!button) {
            button = document.createElement('button');
            button.type = 'button';
            button.className = 'ytp-button yg-pip-button';
            button.style.cssText = 'vertical-align:top;';
            const icon = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
            icon.setAttribute('viewBox', '0 0 24 24');
            icon.setAttribute('aria-hidden', 'true');
            icon.style.cssText = 'width:24px;height:24px;vertical-align:middle;';
            const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
            path.setAttribute('fill', 'currentColor');
            path.setAttribute('d', 'M3 4h18v16H3V4zm2 2v12h14V6H5zm6 6h7v5h-7v-5z');
            icon.appendChild(path); button.appendChild(icon);
            button.addEventListener('click', togglePip);
            controls.prepend(button);
        }
        const active = !!document.pictureInPictureElement;
        const label = (active ? 'Вернуть видео в окно' : 'Картинка в картинке') + ' (Alt+P)';
        // Called on DOM churn; only touch attributes that actually changed.
        if (button.title !== label) {
            button.title = label;
            button.setAttribute('aria-label', label);
            button.setAttribute('aria-pressed', String(active));
        }
        if (button.disabled !== video.disablePictureInPicture) button.disabled = video.disablePictureInPicture;
    }
    document.addEventListener('keydown', function (event) {
        if (!event.altKey || event.ctrlKey || event.metaKey || event.shiftKey || event.repeat || event.defaultPrevented) return;
        const target = event.target;
        if (target && (target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName))) return;
        const video = currentVideo();
        if (!video) return;
        if (event.code === 'KeyP') {
            event.preventDefault(); event.stopPropagation(); togglePip();
        } else if (event.code === 'ArrowUp' || event.code === 'ArrowDown') {
            if (!Number.isFinite(video.duration)) { notify('Скорость прямого эфира управляется YouTube'); return; }
            event.preventDefault(); event.stopPropagation();
            video.playbackRate = Math.min(2, Math.max(0.25, Math.round((video.playbackRate + (event.code === 'ArrowUp' ? 0.25 : -0.25)) * 4) / 4));
            notify('Скорость воспроизведения: ' + video.playbackRate + '×');
        }
    }, true);
    function queueSync() {
        if (queued) return;
        queued = true;
        setTimeout(function () { queued = false; syncControls(); }, 150);
    }
    function start() {
        if (!document.documentElement) return;
        new MutationObserver(queueSync).observe(document.documentElement, { childList: true, subtree: true });
        syncControls();
    }
    if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start, { once: true });
    else start();
    document.addEventListener('yt-navigate-finish', queueSync);
})();

