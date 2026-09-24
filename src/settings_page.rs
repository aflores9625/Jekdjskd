//! In-app settings page served from the `youtube-glass` custom protocol.
//!
//! The page reads the current prefs via `fetch('/state')` (handled on the
//! Rust side by `settings_protocol`) and writes changes back through the
//! IPC channel as `set:<key>=<0|1>` messages. Styled to match the Liquid
//! Glass theme so it doesn't feel like a foreign surface.
//!
//! Toggles split into two groups: "live" ones (take effect immediately via
//! the event loop) and "apply on restart" ones (theme/ads/analytics are
//! init-scripts and can't be removed in place).

/// Serves the settings page. Called from the custom-protocol handler in
/// main.rs. `path` is the request path (without scheme/host), `current_json`
/// is the JSON of the current Settings (already serialized by the caller so
/// `/state` requests return it verbatim without re-reading the file).
pub fn handle(path: &str, current_json: &str) -> Option<(u16, &'static str, Vec<u8>)> {
    // Only two resources: the page itself at `/` (or `/settings`), and the
    // JSON state at `/state`. Anything else 404s (and the page shows an
    // error) rather than serving something stale.
    // Normalized here because the caller strips the leading '/': matching
    // only "/settings" and "/state" made both routes 404, so the page never
    // opened from the tray.
    match path.trim_matches('/') {
        "" | "settings" => Some((200, "text/html; charset=utf-8", page_html().into())),
        "state" => Some((200, "application/json", current_json.as_bytes().to_vec())),
        _ => Some((404, "text/plain", b"not found".to_vec())),
    }
}

#[cfg(test)]
mod tests {
    use super::handle;

    #[test]
    fn routes_accept_paths_with_and_without_leading_slash() {
        for page in ["", "/", "settings", "/settings", "settings/"] {
            assert_eq!(handle(page, "{}").unwrap().0, 200, "{page:?}");
        }
        for state in ["state", "/state"] {
            let (status, mime, body) = handle(state, "{\"theme\":true}").unwrap();
            assert_eq!((status, mime), (200, "application/json"));
            assert_eq!(body, b"{\"theme\":true}");
        }
        assert_eq!(handle("missing", "{}").unwrap().0, 404);
    }
}

/// The page. Inline (no external CSS/JS) so a single custom-protocol response
/// renders it - simpler than wiring up multiple protocol routes for css/js.
fn page_html() -> String {
    format!(
        r##"<!DOCTYPE html>
<html lang="ru" class="ytg-settings">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Настройки — YouTube</title>
<style>
:root {{
  --bg: #0d0d0d;
  --card: rgba(255,255,255,0.04);
  --card-hover: rgba(255,255,255,0.07);
  --border: rgba(255,255,255,0.10);
  --text: #f5f5f5;
  --muted: #b8b8bd;
  color-scheme: dark;
  --accent: #f1f1f1;
  --warn: #e0a800;
  --ease: cubic-bezier(0.22,1,0.36,1);
}}
* {{ box-sizing: border-box; }}
html, body {{ margin: 0; padding: 0; background: var(--bg); color: var(--text);
  font-family: "Roboto","Segoe UI",sans-serif; min-height: 100vh; }}
body {{ background:
  radial-gradient(1200px 600px at 50% -10%, rgba(255,255,255,0.04), transparent 60%),
  linear-gradient(160deg, #101010 0%, #0d0d0d 50%, #090909 100%); }}
.wrap {{ max-width: 720px; margin: 0 auto; padding: clamp(24px, 5vw, 56px) 24px 112px; }}
header {{ margin-bottom: 32px; }}
header h1 {{ font-size: 26px; font-weight: 600; margin: 0 0 6px; }}
header p {{ color: var(--muted); margin: 0; font-size: 14px; }}

.section {{ margin-bottom: 28px; }}
.section-title {{ font-size: 12px; text-transform: uppercase; letter-spacing: 0.6px;
  color: var(--muted); margin: 0 4px 10px; }}

.row {{ display: flex; align-items: flex-start; gap: 16px; background: var(--card);
  border: 1px solid var(--border); border-radius: 14px; padding: 16px 18px;
  margin-bottom: 10px; transition: background 0.22s var(--ease); }}
.row:hover {{ background: var(--card-hover); }}
.row-text {{ flex: 1; min-width: 0; }}
.row-title {{ font-size: 15px; font-weight: 500; display: flex; align-items: center; flex-wrap: wrap; gap: 8px; }}
.row-desc {{ font-size: 13px; color: var(--muted); margin-top: 3px; line-height: 1.6; overflow-wrap: anywhere; }}
.badge {{ font-size: 10px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px;
  color: var(--warn); background: rgba(224,168,0,0.14); padding: 2px 7px; border-radius: 999px;
  border: 1px solid rgba(224,168,0,0.25); }}

/* iOS-style toggle */
.switch {{ position: relative; width: 44px; height: 44px; flex-shrink: 0; }}
.switch input {{ opacity: 0; width: 100%; height: 100%; margin: 0; position: absolute; inset: 0; cursor: pointer; }}
.slider {{ position: absolute; inset: 9px 0; pointer-events: none; background: rgba(255,255,255,0.14);
  border-radius: 999px; transition: background 0.25s var(--ease); cursor: pointer; }}
.slider::before {{ content: ""; position: absolute; width: 20px; height: 20px; left: 3px; top: 3px;
  background: #fff; border-radius: 50%; transition: transform 0.25s var(--ease);
  box-shadow: 0 1px 3px rgba(0,0,0,0.4); }}
.switch input:checked + .slider {{ background: var(--accent); }}
.switch input:checked + .slider::before {{ transform: translateX(18px); }}
.switch input:checked + .slider::before {{ background: #0d0d0d; }}
.switch input:focus-visible + .slider {{ outline: 2px solid #a8d4ff; outline-offset: 4px; }}

.actions {{ display: flex; gap: 10px; margin-top: 24px; flex-wrap: wrap; }}
.btn {{ background: rgba(255,255,255,0.08); color: var(--text); border: none;
  border-radius: 999px; padding: 11px 22px; font-size: 14px; font-weight: 500; cursor: pointer;
  transition: background 0.22s var(--ease), transform 0.22s var(--ease); }}
.btn:not(:disabled):hover {{ background: rgba(255,255,255,0.15); }}
.btn:not(:disabled):active {{ transform: scale(0.97); }}
.btn.primary {{ background: var(--accent); color: #0d0d0d; }}
.btn.primary:not(:disabled):hover {{ background: #fff; }}

.toast {{ position: fixed; bottom: 24px; left: 50%; transform: translateX(-50%) translateY(20px);
  background: rgba(30,30,30,0.95); backdrop-filter: blur(12px); color: var(--text);
  border: 1px solid var(--border); padding: 11px 18px; border-radius: 999px; font-size: 13px;
  opacity: 0; transition: opacity 0.3s var(--ease), transform 0.3s var(--ease); pointer-events: none; }}
.toast.show {{ opacity: 1; transform: translateX(-50%) translateY(0); }}

#err {{ color: #ffb4b4; line-height: 1.6; overflow-wrap: anywhere; }}
.load-state {{ padding: 20px; border: 1px solid var(--border); border-radius: 14px; }}
.load-state p {{ color: var(--muted); }}
.btn {{ min-height: 44px; font: inherit; font-size: 14px; }}
.btn:focus-visible {{ outline: 2px solid #a8d4ff; outline-offset: 4px; }}
.btn:disabled {{ background: #292929; color: #ababab; cursor: not-allowed; }}
.row:focus-within {{ border-color: #819ab2; background: var(--card-hover); }}
.row-title label {{ cursor: pointer; }}
.row-status {{ display: block; font-size: 12px; color: var(--muted); margin-top: 8px; }}
.row[data-enabled="true"] .row-status {{ color: #d1e6d8; }}
.restart-note {{ color: var(--muted); font-size: 13px; line-height: 1.6; margin: 16px 0 0; }}
.restart-note.pending {{ color: #f4d488; }}
.toast {{ max-width: calc(100vw - 32px); width: max-content; text-align: center; border-radius: 16px; z-index: 10; }}
@media (max-width: 480px) {{
  .wrap {{ padding-inline: 16px; }}
  .row {{ padding: 14px; gap: 12px; }}
  .row-title {{ font-size: 16px; }}
  .actions .btn {{ width: 100%; }}
  .badge {{ text-transform: none; letter-spacing: 0; font-size: 11px; }}
}}
@media (prefers-reduced-motion: reduce) {{
  *, *::before, *::after {{ transition: none !important; animation: none !important; scroll-behavior: auto !important; }}
  .btn:active {{ transform: none; }}
}}
@media (forced-colors: active) {{
  .slider {{ border: 1px solid ButtonText; background: Canvas; }}
  .slider::before {{ background: ButtonText; }}
  .switch input:checked + .slider {{ background: Highlight; }}
  .switch input:checked + .slider::before {{ background: HighlightText; }}
  .btn {{ border: 1px solid ButtonText; }}
  .btn:disabled {{ color: GrayText; }}
  .btn:focus-visible, .switch input:focus-visible + .slider {{ outline-color: Highlight; }}
}}
</style>
</head>
<body>
<main class="wrap">
  <header>
    <h1>Настройки</h1>
    <p>YouTube Glass · Внешний вид и поведение приложения.</p>
  </header>

  <div id="content" aria-busy="true">
    <div class="load-state">
      <p id="loading" role="status">Загрузка настроек…</p>
      <p id="err" role="alert"></p>
      <button class="btn" id="retry" hidden>Повторить загрузку</button>
      <button class="btn" id="load-back">← Назад к YouTube</button>
    </div>
  </div>
</main>

<div class="toast" id="toast" role="status" aria-live="polite" aria-atomic="true"></div>

<script>
// Which keys apply live vs need a restart. Drives the badge + the "restart
// pending" hint shown after a change.
var LIVE = {{ always_on_top: 1, close_to_tray: 1 }};
var LABELS = {{
  theme:        {{ t: 'Тема Liquid Glass', d: 'Стеклянная монохромная тема оформления YouTube.' }},
  block_ads:    {{ t: 'Скрытие рекламных баннеров', d: 'Скрывать рекламные баннеры и рекламу в ленте (без пропуска видео-рекламы).' }},
  analytics:    {{ t: 'Аналитика видео', d: 'Показывать панель со статистикой (просмотры, лайки, дизлайки, вовлечённость) на странице просмотра.' }},
  cinema:       {{ t: 'Режим кинотеатра', d: 'Включать затемнение вокруг плеера при запуске. Кнопка с иконкой луны в панели плеера всегда доступна для переключения.' }},
  prefer_hd:    {{ t: 'Предпочитать 1080p HD', d: 'Записывает предпочтение качества 1080p в профиль YouTube. YouTube может понизить качество при медленном интернете или если ролик не имеет 1080p.' }},
  discord_rpc:  {{ t: 'Discord Rich Presence', d: 'Показывать «Смотрит: название видео» с обложкой в статусе Discord.' }},
  always_on_top:{{ t: 'Поверх всех окон', d: 'Окно остаётся поверх других окон во время работы.' }},
  close_to_tray:{{ t: 'Сворачивать в трей при закрытии', d: 'Кнопка закрытия прячет окно в трей вместо выхода. Выход — через меню трея.' }}
}};
var state = null;
var initialState = null;
var restartNeeded = false;

function render() {{
  if (!state) return;
  // Trusted Types: YouTube's policy still applies on this origin, so build
  // via the DOM rather than innerHTML. (This page is served from a different
  // origin than youtube.com, but the init-script runs document-wide.)
  var content = document.getElementById('content');
  while (content.firstChild) content.removeChild(content.firstChild);

  var live = ['always_on_top','close_to_tray'];
  var restart = ['theme','block_ads','analytics','cinema','prefer_hd','discord_rpc'];
  var mk = function (keys) {{
    var frag = document.createDocumentFragment();
    keys.forEach(function (k) {{
      var rowEl = document.createElement('div');
      rowEl.className = 'row';
      rowEl.dataset.enabled = String(!!state[k]);
      var txt = document.createElement('div');
      txt.className = 'row-text';
      var t = document.createElement('div');
      t.className = 'row-title';
      var titleLabel = document.createElement('label');
      titleLabel.htmlFor = 'setting-' + k;
      titleLabel.id = 'title-' + k;
      titleLabel.textContent = LABELS[k].t;
      t.appendChild(titleLabel);
      if (!LIVE[k]) {{
        var b = document.createElement('span');
        b.className = 'badge';
        b.textContent = 'применится после перезапуска';
        t.appendChild(b);
      }}
      var d = document.createElement('div');
      d.className = 'row-desc';
      d.id = 'desc-' + k;
      d.textContent = LABELS[k].d;
      // Discord RPC needs an application id; until it's set in the settings
      // file the toggle is saved but the feature stays dormant - say so.
      if (k === 'discord_rpc' && !state.discord_configured) {{
        d.textContent += ' Требуется Application ID: создайте приложение на ' +
          'discord.com/developers, скопируйте ID и добавьте строку ' +
          'discord_client_id=<ID> в %APPDATA%\\YoutubeGlass\\settings.';
      }}
      txt.appendChild(t); txt.appendChild(d);
      var status = document.createElement('span');
      status.className = 'row-status';
      status.id = 'status-' + k;
      status.textContent = state[k] ? 'Включено' : 'Выключено';
      txt.appendChild(status);
      var sw = document.createElement('label');
      sw.className = 'switch';
      var inp = document.createElement('input');
      inp.type = 'checkbox';
      inp.id = 'setting-' + k;
      inp.setAttribute('aria-labelledby', 'title-' + k);
      inp.setAttribute('aria-describedby', 'desc-' + k);
      inp.dataset.key = k;
      inp.checked = !!state[k];
      inp.addEventListener('change', function () {{ onToggle(k, inp.checked); }});
      var sl = document.createElement('span');
      sl.className = 'slider';
      sl.setAttribute('aria-hidden', 'true');
      sw.appendChild(inp); sw.appendChild(sl);
      rowEl.appendChild(txt); rowEl.appendChild(sw);
      frag.appendChild(rowEl);
    }});
    return frag;
  }};

  function section(titleText, keys) {{
    var sec = document.createElement('section');
    sec.className = 'section';
    var st = document.createElement('h2');
    st.className = 'section-title';
    st.textContent = titleText;
    sec.appendChild(st);
    sec.appendChild(mk(keys));
    content.appendChild(sec);
  }}

  section('Действует сразу', live);
  section('Применится после перезапуска', restart);

  var actions = document.createElement('div');
  actions.className = 'actions';
  var back = document.createElement('button');
  back.className = 'btn';
  back.textContent = '← Назад к YouTube';
  back.onclick = function () {{ if (window.ipc) window.ipc.postMessage('back'); }};
  var restartBtn = document.createElement('button');
  restartBtn.className = 'btn primary';
  restartBtn.textContent = 'Перезапустить';
  restartBtn.disabled = true;
  restartBtn.id = 'restart-btn';
  restartBtn.onclick = function () {{ if (window.ipc) window.ipc.postMessage('restart'); }};
  actions.appendChild(back);
  actions.appendChild(restartBtn);
  content.appendChild(actions);
  var note = document.createElement('p');
  note.id = 'restart-note';
  note.className = 'restart-note';
  note.textContent = 'Изменения сохраняются автоматически. Перезапуск пока не требуется.';
  restartBtn.setAttribute('aria-describedby', note.id);
  content.appendChild(note);
}}

function onToggle(key, on) {{
  var input = document.getElementById('setting-' + key);
  try {{
    if (!window.ipc) throw new Error('IPC unavailable');
    window.ipc.postMessage('set:' + key + '=' + (on ? 1 : 0));
  }} catch (error) {{
    input.checked = !!state[key];
    toast('Не удалось передать изменение. Попробуйте ещё раз.');
    return;
  }}
  state[key] = on;
  input.closest('.row').dataset.enabled = String(on);
  document.getElementById('status-' + key).textContent = on ? 'Включено' : 'Выключено';
  restartNeeded = Object.keys(LABELS).some(function (name) {{
    return !LIVE[name] && !!state[name] !== !!initialState[name];
  }});
  document.getElementById('restart-btn').disabled = !restartNeeded;
  var note = document.getElementById('restart-note');
  note.classList.toggle('pending', restartNeeded);
  note.textContent = restartNeeded
    ? 'Есть изменения, требующие перезапуска приложения.'
    : 'Изменения сохраняются автоматически. Перезапуск пока не требуется.';
  toast(LABELS[key].t + ': ' + (on ? 'включено.' : 'выключено.') +
    (!LIVE[key] && restartNeeded ? ' Требуется перезапуск.' : ''));

}}

var toastTimer = null;
function toast(msg) {{
  var t = document.getElementById('toast');
  t.textContent = msg;
  t.classList.add('show');
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(function () {{ t.classList.remove('show'); }}, 2200);
}}

document.getElementById('load-back').onclick = function () {{
  if (window.ipc) window.ipc.postMessage('back');
}};
document.getElementById('retry').onclick = function () {{ loadState(true); }};
function loadState(restoreFocus) {{
  document.getElementById('content').setAttribute('aria-busy', 'true');
  document.getElementById('loading').textContent = 'Загрузка настроек…';
  document.getElementById('err').textContent = '';
  document.getElementById('retry').hidden = true;
fetch('state').then(function (r) {{
  if (!r.ok) throw new Error('state http ' + r.status);
  return r.json();
}}).then(function (s) {{
  state = s;
  initialState = Object.assign({{}}, s);
  document.getElementById('err').textContent = '';
  render();
  document.getElementById('content').setAttribute('aria-busy', 'false');
  if (restoreFocus) document.querySelector('input').focus();
}}).catch(function (e) {{
  document.getElementById('content').setAttribute('aria-busy', 'false');
  document.getElementById('loading').textContent = 'Настройки недоступны';
  document.getElementById('retry').hidden = false;
  document.getElementById('err').textContent = 'Не удалось загрузить настройки: ' + e.message;
  if (restoreFocus) document.getElementById('retry').focus();
}});
}}
loadState(false);
</script>
</body>
</html>"##
    )
}
