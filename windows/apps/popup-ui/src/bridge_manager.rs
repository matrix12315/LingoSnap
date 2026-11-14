//! Manager-page data bridge: populates the fair-copy `manager.html` DOM from
//! live manager state and posts page commands back as `{"mcmd": ...}` web
//! messages. Payloads are plain JSON; the host forwards them verbatim.

/// App mode for the manager sheet: the shell card IS the window (no desk
/// padding, no caption row, transparent corners), and the title bar gains
/// the minimize/close controls a borderless window needs.
pub const MANAGER_APP_CSS: &str = r#"
html, body { background: transparent !important; margin: 0 !important; padding: 0 !important;
  width: max-content !important; height: max-content !important; overflow: hidden !important; }
body { display: block !important; }
.caprow, .themebtn { display: none !important; }
.card.shell { box-shadow: none !important; width: 860px !important; }
/* The window is exactly the shell's 860px, so the mockup's narrow-viewport
   rule (max-width: 900px) would collapse every grid to one column. Keep the
   designed desktop layout. */
#page-settings .fields2, .edgrid { grid-template-columns: 1fr 1fr !important; }
.hsplit { grid-template-columns: 272px 1fr !important; }
.titlebar { cursor: default; user-select: none; -webkit-user-select: none; }
.titlebar .winbtns { display: flex; gap: 6px; margin-left: 12px; }
.titlebar .winbtns .iconbtn { width: 24px; height: 24px; }
.savenote.err { color: var(--seal-deep); }
"#;

/// The manager page is the mockup sheet; this bridge fills it with the live
/// configuration and turns its controls into the same operations the native
/// manager performs (the host routes each command through the native
/// validation and save paths, then pushes fresh snapshots and a status line).
pub const MANAGER_BRIDGE_JS: &str = r##"<script>
(function () {
  if (window.LINGO_MANAGER) return;
  function esc(text) {
    var d = document.createElement('div');
    d.textContent = text == null ? '' : String(text);
    return d.innerHTML;
  }
  function post(msg) {
    try { window.chrome.webview.postMessage(JSON.stringify(msg)); } catch (e) {}
  }
  function set(id, value) {
    var el = document.getElementById(id);
    if (el) el.value = value == null ? '' : String(value);
  }
  function get(id) {
    var el = document.getElementById(id);
    return el ? el.value : '';
  }
  function q(sel, root) { return (root || document).querySelector(sel); }
  function lens(root) {
    if (window.liquidLens) requestAnimationFrame(function () { window.liquidLens(root); });
  }
  var CHEVRON = '<svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"><path d="M2 3.5l3 3 3-3"/></svg>';

  // —— Status: the native manager's status line, shown as the page note ——
  var noteTimer = null;
  function showStatus(message, ok) {
    if (!message) return;
    var page = document.querySelector('.mpage:not([hidden])');
    var note = page && page.querySelector('.savenote');
    if (!note) return;
    note.textContent = (ok ? '\u2713 ' : '') + message;
    note.classList.toggle('err', !ok);
    note.classList.add('show');
    clearTimeout(noteTimer);
    noteTimer = setTimeout(function () { note.classList.remove('show'); }, ok ? 2200 : 6000);
  }

  // —— Dropdowns: the mockup's .select fields open the paper .pmenu list ——
  var openMenus = [];
  function closeMenus() {
    openMenus.forEach(function (m) { m.hidden = true; m.parentNode.classList.remove('open'); });
    openMenus = [];
  }
  document.addEventListener('click', function (e) {
    if (!e.target.closest('.select')) closeMenus();
  });
  function choice(input, options, selected, onPick) {
    if (!input) return;
    var box = input.closest('.select');
    if (!box) return;
    input.readOnly = true;
    input.style.cursor = 'pointer';
    var menu = box.querySelector('.pmenu');
    if (!menu) {
      menu = document.createElement('div');
      menu.className = 'pmenu';
      menu.setAttribute('role', 'listbox');
      menu.hidden = true;
      box.appendChild(menu);
      box.addEventListener('click', function (e) {
        if (e.target.closest('.pmenu')) return;
        var opening = menu.hidden;
        closeMenus();
        if (opening) { menu.hidden = false; box.classList.add('open'); openMenus.push(menu); }
      });
    }
    menu.innerHTML = '';
    options.forEach(function (opt, index) {
      var item = document.createElement('div');
      item.className = 'pitem' + (index === selected ? ' on' : '');
      item.textContent = opt.label;
      item.addEventListener('click', function (e) {
        e.stopPropagation();
        closeMenus();
        input.value = opt.label;
        menu.querySelectorAll('.pitem').forEach(function (it) { it.classList.remove('on'); });
        item.classList.add('on');
        onPick(opt, index);
      });
      menu.appendChild(item);
    });
    if (selected >= 0 && options[selected]) input.value = options[selected].label;
  }

  // —— Settings ——
  var current = { profiles: [], selection: '', hover: '', language: 'en' };
  var settingsTimer = null;
  function postSettings() {
    clearTimeout(settingsTimer);
    settingsTimer = setTimeout(function () {
      var order = [];
      document.querySelectorAll('#orderlist .orderrow').forEach(function (row) {
        if (row.dataset.id) order.push(row.dataset.id);
      });
      post({
        mcmd: 'save_settings',
        endpoint: get('endpoint'),
        model: get('model'),
        credtarget: get('credtarget'),
        hkSelection: get('hkSelection'),
        hkHover: get('hkHover'),
        hkPause: get('hkPause'),
        hkPopup: get('hkPopup'),
        selectionDefault: current.selection,
        hoverDefault: current.hover,
        order: order
      });
    }, 600);
  }
  function hotkeyText(spec) {
    if (!spec) return '';
    return spec.split('+').map(function (p) { return p.trim(); })
      .filter(function (p) { return p.length; }).join(' + ');
  }
  // Users pick profiles by name; the ID is an internal code and is never shown.
  function profileName(p) { return p.name && p.name.trim() ? p.name.trim() : p.id; }
  function buildThemeChoice() {
    var themes = [{ id: 'light', label: t('themeLight') }, { id: 'dark', label: t('themeDark') }];
    choice(document.getElementById('uitheme'), themes, current.theme === 'dark' ? 1 : 0, function (opt) {
      if (opt.id !== current.theme) {
        current.theme = opt.id;
        if (window.setTheme) window.setTheme(opt.id);
        post({ mcmd: 'save_theme', theme: opt.id });
      }
    });
  }
  function buildAutostartChoice() {
    var opts = [{ id: 'off', label: t('optOff') }, { id: 'on', label: t('optOn') }];
    choice(document.getElementById('autostart'), opts, current.autostart ? 1 : 0, function (opt) {
      current.autostart = opt.id === 'on';
      post({ mcmd: 'save_autostart', enabled: current.autostart });
    });
  }
  function applySettings(cfg) {
    set('endpoint', cfg.endpoint);
    set('model', cfg.model);
    set('credtarget', cfg.credtarget);
    set('hkSelection', hotkeyText(cfg.hkSelection));
    set('hkHover', hotkeyText(cfg.hkHover));
    set('hkPause', hotkeyText(cfg.hkPause));
    set('hkPopup', hotkeyText(cfg.hkPopup));
    current.profiles = cfg.profiles || [];
    current.selection = cfg.selectionId || '';
    current.hover = cfg.hoverId || '';
    current.language = cfg.language || 'en';
    if (window.setLang) window.setLang(current.language);
    var opts = current.profiles.map(function (p) { return { id: p.id, label: profileName(p) }; });
    var idx = function (id) { return opts.findIndex(function (o) { return o.id === id; }); };
    choice(document.getElementById('selfprof'), opts, idx(current.selection), function (opt) {
      current.selection = opt.id; postSettings();
    });
    choice(document.getElementById('hoverprof'), opts, idx(current.hover), function (opt) {
      current.hover = opt.id; postSettings();
    });
    var langs = [{ id: 'en', label: 'English' }, { id: 'zh-CN', label: '\u7b80\u4f53\u4e2d\u6587' }];
    choice(document.getElementById('uilang'), langs, current.language === 'zh-CN' ? 1 : 0, function (opt) {
      if (opt.id !== current.language) { current.language = opt.id; post({ mcmd: 'save_language', language: opt.id }); }
    });
    current.autostart = !!cfg.autostart;
    current.theme = cfg.theme === 'dark' ? 'dark' : 'light';
    if (window.setTheme) window.setTheme(current.theme);
    buildThemeChoice();
    buildAutostartChoice();
    var note = q('#page-settings .group .actions .statnote');
    if (note && cfg.credentialStatus) {
      note.innerHTML = '<i style="' + (cfg.credentialPresent ? '' : 'background:transparent;border:1px solid var(--rule)') + '"></i>' + esc(cfg.credentialStatus);
    }
    buildOrder(current.profiles, cfg.order || []);
    buildHistoryFilters();
  }

  var GRIP = '<svg width="8" height="14" viewBox="0 0 8 14" fill="currentColor"><circle cx="2" cy="2" r="1.3"/><circle cx="6" cy="2" r="1.3"/><circle cx="2" cy="7" r="1.3"/><circle cx="6" cy="7" r="1.3"/><circle cx="2" cy="12" r="1.3"/><circle cx="6" cy="12" r="1.3"/></svg>';
  var UP_SVG = '<svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"><path d="M2 6.5l3-3 3 3"/></svg>';
  var DOWN_SVG = CHEVRON;
  function buildOrder(profiles, orderIds) {
    var list = document.getElementById('orderlist');
    if (!list) return;
    var byId = {};
    profiles.forEach(function (p) { byId[p.id] = profileName(p); });
    var ordered = orderIds.filter(function (id) { return byId[id] !== undefined; })
      .map(function (id) { return { id: id, name: byId[id] }; });
    profiles.forEach(function (p) {
      if (!ordered.some(function (o) { return o.id === p.id; })) ordered.push(p);
    });
    // Keep the page's own divider node: the mockup's reorder script holds a
    // reference to it and re-inserts it after the fourth row.
    var divider = list.querySelector('.odivider');
    if (!divider) { divider = document.createElement('div'); divider.className = 'odivider'; divider.setAttribute('data-i18n', 'beneathMore'); }
    divider.textContent = t('beneathMore');
    list.innerHTML = '';
    ordered.forEach(function (p, index) {
      var row = document.createElement('div');
      row.className = 'orderrow';
      row.dataset.id = p.id;
      row.innerHTML = '<span class="ogrip" data-i18n-title="dragReorder" title="' + esc(t('dragReorder')) + '">' + GRIP + '</span>' +
        '<span class="oname">' + esc(profileName(p)) + '</span>' +
        '<span class="omove">' +
        '<button class="iconbtn up" type="button" data-i18n-title="moveUp" data-i18n-aria="moveUp" title="' + esc(t('moveUp')) + '" aria-label="' + esc(t('moveUp')) + '">' + UP_SVG + '</button>' +
        '<button class="iconbtn down" type="button" data-i18n-title="moveDown" data-i18n-aria="moveDown" title="' + esc(t('moveDown')) + '" aria-label="' + esc(t('moveDown')) + '">' + DOWN_SVG + '</button>' +
        '</span>';
      row.querySelector('.up').disabled = index === 0;
      row.querySelector('.down').disabled = index === ordered.length - 1;
      list.appendChild(row);
      if (index === 3 && ordered.length > 4) list.appendChild(divider);
    });
    lens(list);
  }

  // —— Prompts ——
  var promptId = '';
  function applyPrompts(payload) {
    var menu = document.getElementById('pidMenu');
    if (menu) {
      menu.innerHTML = '';
      var picks = (payload.profiles || []).map(function (p) { return { id: p.id, name: profileName(p) }; });
      // An unsaved draft is listed under its own (so far) name.
      if (payload.selectedId && !picks.some(function (p) { return p.id === payload.selectedId; })) {
        picks.push({ id: payload.selectedId, name: profileName(payload.selected || { id: payload.selectedId }) });
      }
      picks.forEach(function (pick) {
        var id = pick.id;
        var item = document.createElement('div');
        item.className = 'pitem' + (id === payload.selectedId ? ' on' : '');
        item.dataset.v = id;
        item.textContent = pick.name;
        item.addEventListener('click', function (e) {
          // The mockup's own menu handler would load its sample prompts.
          e.stopPropagation();
          menu.hidden = true;
          var box = document.getElementById('pidbox');
          if (box) box.classList.remove('open');
          post({ mcmd: 'select_prompt', id: id });
        });
        menu.appendChild(item);
      });
    }
    promptId = payload.selectedId || '';
    set('pid', promptId);
    var sel = payload.selected || {};
    set('pname', sel.name || '');
    set('pmodel', sel.model || '');
    set('ptemp', sel.temp == null ? '' : sel.temp);
    set('ptokens', sel.tokens == null ? '' : sel.tokens);
    set('sysprompt', sel.sys || '');
    set('usertpl', sel.user || '');
    var save = document.getElementById('savePromptBtn');
    var restore = document.getElementById('restoreBtn');
    if (save) save.disabled = !payload.draft;
    if (restore) restore.disabled = !payload.draft;
    // An unsaved draft is discarded with Restore, not deleted.
    var del = document.getElementById('deletePromptBtn');
    if (del) del.disabled = !!payload.draft || !payload.selectedId;
  }
  function postPrompt() {
    post({
      mcmd: 'save_prompt',
      id: get('pid'),
      name: get('pname'),
      model: get('pmodel'),
      temp: get('ptemp'),
      tokens: get('ptokens'),
      sys: get('sysprompt'),
      user: get('usertpl')
    });
  }

  // —— History ——
  var history = { entries: [], selected: 0, search: '', prompt: 0, source: 0, order: 0 };
  function localTime(utc) {
    var d = new Date(utc);
    if (isNaN(d.getTime())) return String(utc || '').slice(11, 16);
    var pad = function (n) { return (n < 10 ? '0' : '') + n; };
    var now = new Date();
    var hm = pad(d.getHours()) + ':' + pad(d.getMinutes());
    if (d.toDateString() === now.toDateString()) return hm;
    return (d.getMonth() + 1) + '-' + pad(d.getDate()) + ' ' + hm;
  }
  function postQuery() {
    post({ mcmd: 'history_query', search: history.search, prompt: history.prompt,
      source: history.source, order: history.order });
  }
  function buildHistoryFilters() {
    var boxes = document.querySelectorAll('#page-history .htoolbar .select');
    if (boxes.length < 3) return;
    var prompts = [{ label: t('allProfiles') }].concat(current.profiles.map(function (p) { return { label: profileName(p) }; }));
    choice(q('input', boxes[0]), prompts, Math.min(history.prompt, prompts.length - 1), function (o, i) { history.prompt = i; postQuery(); });
    var sources = ['allSources', 'srcSelection', 'srcHover', 'srcClipboard', 'srcOcr'].map(function (k) { return { label: t(k) }; });
    choice(q('input', boxes[1]), sources, history.source, function (o, i) { history.source = i; postQuery(); });
    var orders = ['newest', 'oldest'].map(function (k) { return { label: t(k) }; });
    choice(q('input', boxes[2]), orders, history.order, function (o, i) { history.order = i; postQuery(); });
  }
  function entriesText(n) { return t('entries').replace('{n}', n); }
  function applyHistory(payload) {
    history.entries = payload.entries || [];
    history.selected = payload.selected || 0;
    var stat = q('#page-history .pagehead .statnote');
    history.total = payload.total || 0;
    if (stat) stat.textContent = entriesText(history.total);
    var list = q('#page-history .hlist');
    if (!list) return;
    list.innerHTML = '';
    history.entries.forEach(function (entry, index) {
      var item = document.createElement('div');
      item.className = 'hitem' + (index === history.selected ? ' on' : '');
      item.innerHTML =
        '<div class="htop"><span class="ht">' + esc(entry.target) + '</span>' +
        '<span class="htime">' + esc(localTime(entry.utc)) + '</span></div>' +
        '<div class="hp">' + esc(entry.preview) + '</div>' +
        '<div class="hmeta"><span class="hprof">' + esc(entry.prompt) + '</span>' +
        '<span class="hsrc">' + esc(entry.source) + '</span></div>';
      item.addEventListener('click', function () {
        list.querySelectorAll('.hitem').forEach(function (el) { el.classList.remove('on'); });
        item.classList.add('on');
        history.selected = index;
        post({ mcmd: 'select_history', index: index });
      });
      list.appendChild(item);
    });
    var detail = q('#page-history .hdetail');
    if (detail) detail.style.visibility = history.entries.length ? '' : 'hidden';
  }
  // Inline Markdown as the popup renders it: **bold**, *em*, `code`.
  function inline(text) {
    var parts = String(text).split('`'), out = '';
    for (var i = 0; i < parts.length; i++) {
      if (i % 2 === 1 && i < parts.length - 1) { out += '<span class="alt">' + esc(parts[i]) + '</span>'; continue; }
      out += esc((i % 2 === 1) ? '`' + parts[i] : parts[i])
        .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
        .replace(/(^|[^*\w])\*([^*\s][^*]*)\*(?!\*)/g, '$1<em>$2</em>');
    }
    return out;
  }
  // Field labels and empty answers, shared by the popup and History
  // renderers (keep both copies identical). "Chinese: 你好" becomes a muted
  // label plus an ink value (at most 3 words before the colon); a value that
  // is only "None" becomes a muted, italic placeholder.
  function fieldHtml(text) {
    var s = String(text).trim();
    function value(v) {
      return /^none\.?$/i.test(v) ? '<span class="none">None</span>' : inline(v);
    }
    var m = s.match(/^(?:\*\*)?([^:\uFF1A*`]+?)(?:\*\*)?(?::(?:\*\*)?(?:\s+|$)|\uFF1A(?:\*\*)?\s*)([\s\S]*)$/);
    if (m) {
      var label = m[1].trim();
      if (label.length && label.length <= 40 && label.split(/\s+/).length <= 3) {
        return '<span class="flabel">' + esc(label) + '</span>' + value(m[2].trim());
      }
    }
    return value(s);
  }
  function renderOutput(markdown) {
    var out = q('#page-history .houtput');
    if (!out) return;
    out.innerHTML = '';
    var current = null;
    var sections = [];
    (markdown || '').split(/\r?\n/).forEach(function (line) {
      var t = line.trim();
      if (!t.length) return;
      var h = t.match(/^#{1,6}\s+(.*)$/);
      if (h) {
        current = { title: h[1], items: [], prose: [] };
        sections.push(current);
        return;
      }
      if (!current) { current = { title: null, items: [], prose: [] }; sections.push(current); }
      var li = t.match(/^(?:[-*+•]|\d+[.)])\s+(.*)$/);
      if (li) current.items.push(li[1]); else current.prose.push(t);
    });
    sections.forEach(function (sec, index) {
      var div = document.createElement('div');
      div.className = 'sec';
      var html = '';
      if (sec.title) html += '<h3>' + inline(sec.title) + '</h3>';
      if (sec.items.length) {
        html += '<ul>' + sec.items.map(function (i) { return '<li>' + fieldHtml(i) + '</li>'; }).join('') + '</ul>';
      }
      sec.prose.forEach(function (p) { html += '<p class="' + (index === 0 ? 'trans' : 'fineprint') + '">' + fieldHtml(p) + '</p>'; });
      div.innerHTML = html;
      out.appendChild(div);
    });
  }
  function applyHistoryDetail(payload) {
    var entry = payload.entry || {};
    var head = q('#page-history .hhead');
    if (head) {
      head.innerHTML = '<span class="hprof">' + esc(entry.prompt) + '</span>' +
        '<span class="htime">' + esc(localTime(entry.utc) + ' \u00b7 ' + entry.source) + '</span>';
    }
    var sp = q('#page-history .sourcepanel');
    if (sp) {
      var t = sp.querySelector('.target');
      var c = sp.querySelector('.context');
      if (t) t.textContent = entry.target || '';
      if (c) { c.textContent = entry.context || ''; c.style.display = entry.context ? '' : 'none'; }
    }
    renderOutput(entry.output);
  }

  // setLang relabels static markup; rebuild the controls this script builds.
  document.addEventListener('langchange', function () {
    buildThemeChoice();
    buildAutostartChoice();
    buildHistoryFilters();
    if (current.profiles.length) buildOrder(current.profiles, orderIdsNow());
    var stat = q('#page-history .pagehead .statnote');
    if (stat && history.total != null) stat.textContent = entriesText(history.total);
  });
  function orderIdsNow() {
    var ids = [];
    document.querySelectorAll('#orderlist .orderrow').forEach(function (row) { if (row.dataset.id) ids.push(row.dataset.id); });
    return ids;
  }

  window.LINGO_MANAGER = {
    applyManager: function (json) {
      var data;
      try { data = (typeof json === 'string') ? JSON.parse(json) : json; } catch (e) { return; }
      var payload = data.apply || data;
      switch (payload.kind) {
        case 'settings': applySettings(payload.config || {}); announceReady(); break;
        case 'prompts': applyPrompts(payload); break;
        case 'history': applyHistory(payload); break;
        case 'history_detail': applyHistoryDetail(payload); break;
        case 'status': showStatus(payload.message, payload.ok !== false); break;
        default: break;
      }
    },
    savePrompt: postPrompt
  };

  function wire() {
    // Borderless window chrome: drag by the title bar; minimize and close.
    var bar = q('.titlebar');
    if (bar) {
      var btns = document.createElement('span');
      btns.className = 'winbtns';
      btns.innerHTML =
        '<button class="iconbtn" type="button" data-win="minimize" data-i18n-title="minimize" data-i18n-aria="minimize" title="' + esc(t('minimize')) + '" aria-label="' + esc(t('minimize')) + '">' +
        '<svg width="11" height="11" viewBox="0 0 11 11" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"><path d="M2 5.5h7"/></svg></button>' +
        '<button class="iconbtn" type="button" data-win="close" data-i18n-title="close" data-i18n-aria="close" title="' + esc(t('close')) + '" aria-label="' + esc(t('close')) + '">' +
        '<svg width="11" height="11" viewBox="0 0 13 13" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"><path d="M2.5 2.5l8 8M10.5 2.5l-8 8"/></svg></button>';
      bar.appendChild(btns);
      btns.addEventListener('click', function (e) {
        var b = e.target.closest('button');
        if (b) post({ mcmd: 'window_' + b.dataset.win });
      });
      var drag = null;
      bar.addEventListener('mousedown', function (e) {
        if (e.button !== 0 || e.target.closest('button')) return;
        drag = { x: e.screenX, y: e.screenY };
      });
      window.addEventListener('mousemove', function (e) {
        if (!drag) return;
        if (Math.abs(e.screenX - drag.x) + Math.abs(e.screenY - drag.y) > 3) {
          drag = null;
          post({ cmd: 'begin_drag' });
        }
      });
      window.addEventListener('mouseup', function () { drag = null; });
      lens(bar);
    }
    // Pages hidden at load got no glass lens; give it when they appear.
    document.querySelectorAll('#mnav button').forEach(function (btn) {
      btn.addEventListener('click', function () {
        lens(document.getElementById('page-' + btn.dataset.page));
        if (btn.dataset.page === 'history') post({ mcmd: 'refresh_history' });
      });
    });
    // A status note for the Prompts and History pages (Settings has one).
    ['prompts', 'history'].forEach(function (name) {
      var head = q('#page-' + name + ' .pagehead');
      if (head && !head.querySelector('.savenote')) {
        var note = document.createElement('span');
        note.className = 'savenote';
        var h2 = head.querySelector('h2');
        if (h2 && h2.nextSibling) head.insertBefore(note, h2.nextSibling); else head.appendChild(note);
      }
    });
    // Settings: editable fields auto-save.
    ['endpoint', 'model', 'credtarget', 'hkSelection', 'hkHover', 'hkPause', 'hkPopup']
      .forEach(function (id) {
        var el = document.getElementById(id);
        if (el) el.addEventListener('change', postSettings);
      });
    // Credentials: the key travels only to the manager, which stores it in
    // Windows Credential Manager; the field is cleared right away.
    var keyActions = q('#page-settings .group .actions');
    if (keyActions) {
      var saveKey = q('.btn.primary', keyActions);
      var deleteKey = q('.btn.danger', keyActions);
      if (saveKey) saveKey.addEventListener('click', function () {
        var key = document.getElementById('apikey');
        post({ mcmd: 'save_key', target: get('credtarget'), key: key ? key.value : '' });
        if (key) key.value = '';
      });
      if (deleteKey) deleteKey.addEventListener('click', function () {
        post({ mcmd: 'delete_key', target: get('credtarget') });
      });
    }
    // Profile order: post after any reorder (buttons or drag).
    var list = document.getElementById('orderlist');
    if (list) {
      list.addEventListener('click', function (e) {
        if (e.target.closest('button.iconbtn')) postSettings();
      });
      list.addEventListener('drop', postSettings);
    }
    var keyPage = document.getElementById('keyPageBtn');
    if (keyPage) keyPage.addEventListener('click', function () {
      post({ mcmd: 'open_provider_page', endpoint: get('endpoint') });
    });
    var newPrompt = document.getElementById('newPromptBtn');
    if (newPrompt) newPrompt.addEventListener('click', function () { post({ mcmd: 'new_prompt' }); });
    var savePrompt = document.getElementById('savePromptBtn');
    if (savePrompt) savePrompt.addEventListener('click', postPrompt);
    // Restore = discard the edits and reload the saved prompt.
    var delProfile = document.getElementById('deletePromptBtn');
    if (delProfile) delProfile.addEventListener('click', function () {
      if (promptId) post({ mcmd: 'delete_prompt', id: promptId });
    });
    var restore = document.getElementById('restoreBtn');
    if (restore) restore.addEventListener('click', function () {
      post({ mcmd: 'select_prompt', id: promptId });
    });
    var refresh = q('#page-history .htoolbar .btn');
    if (refresh) refresh.addEventListener('click', function () { post({ mcmd: 'refresh_history' }); });
    var search = q('#page-history .hsearch input');
    if (search) {
      var timer = null;
      search.addEventListener('input', function () {
        clearTimeout(timer);
        timer = setTimeout(function () { history.search = search.value; postQuery(); }, 400);
      });
    }
    var detailActions = q('#page-history .hdetail .actions');
    if (detailActions) {
      var copy = q('.btn.primary', detailActions);
      var del = q('.btn.danger', detailActions);
      if (copy) copy.addEventListener('click', function () { post({ mcmd: 'copy_history', index: history.selected }); });
      if (del) del.addEventListener('click', function () { post({ mcmd: 'delete_history', index: history.selected }); });
    }
  }
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', wire);
  } else {
    wire();
  }
  // The window appears once the page has painted the user's real settings
  // (not the sheet's sample values), so opening the manager never flashes
  // an empty window or mockup data.
  var announced = false;
  function announceReady() {
    if (announced) return;
    announced = true;
    requestAnimationFrame(function () {
      requestAnimationFrame(function () { post({ mcmd: 'window_ready' }); });
    });
  }
})();
</script>"##;
