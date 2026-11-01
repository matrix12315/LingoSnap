//! Manager-page data bridge: populates the fair-copy `manager.html` DOM from
//! live manager state and posts page commands back as `{"mcmd": ...}` web
//! messages. Payloads are plain JSON; the host forwards them verbatim.

pub const MANAGER_BRIDGE_JS: &str = r#"<script>
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

  // —— Settings ——
  var settingsTimer = null;
  function postSettings() {
    clearTimeout(settingsTimer);
    settingsTimer = setTimeout(function () {
      var order = [];
      document.querySelectorAll('#orderlist .orderrow .oid').forEach(function (el) {
        order.push(el.textContent.trim());
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
        order: order
      });
    }, 600);
  }

  function hotkeyText(spec) {
    if (!spec) return '';
    return spec.split('+').map(function (p) { return p.trim(); })
      .filter(function (p) { return p.length; }).join(' + ');
  }

  function applySettings(cfg) {
    set('endpoint', cfg.endpoint);
    set('model', cfg.model);
    set('credtarget', cfg.credtarget);
    set('hkSelection', hotkeyText(cfg.hkSelection));
    set('hkHover', hotkeyText(cfg.hkHover));
    set('hkPause', hotkeyText(cfg.hkPause));
    set('hkPopup', hotkeyText(cfg.hkPopup));
    set('uilang', cfg.language === 'zh-CN' ? '简体中文' : 'English');
    set('selfprof', cfg.selectionDefault || '');
    set('hoverprof', cfg.hoverDefault || '');
    buildOrder(cfg.profiles || [], cfg.order || []);
  }

  var UP_SVG = '<svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"><path d="M2 6.5l3-3 3 3"/></svg>';
  var DOWN_SVG = '<svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"><path d="M2 3.5l3 3 3-3"/></svg>';

  function buildOrder(profiles, orderIds) {
    var list = document.getElementById('orderlist');
    if (!list) return;
    var byId = {};
    profiles.forEach(function (p) { byId[p.id] = p.name; });
    var ordered = orderIds.filter(function (id) { return byId[id]; })
      .map(function (id) { return { id: id, name: byId[id] }; });
    profiles.forEach(function (p) {
      if (!ordered.some(function (o) { return o.id === p.id; })) ordered.push(p);
    });
    list.innerHTML = '';
    ordered.forEach(function (p, index) {
      var row = document.createElement('div');
      row.className = 'orderrow';
      row.dataset.id = p.id;
      row.innerHTML = '<span class="oname">' + esc(p.name) + '</span>' +
        '<span class="oid">' + esc(p.id) + '</span>' +
        '<span class="omove">' +
        '<button class="iconbtn up" type="button" title="Move up" aria-label="Move up">' + UP_SVG + '</button>' +
        '<button class="iconbtn down" type="button" title="Move down" aria-label="Move down">' + DOWN_SVG + '</button>' +
        '</span>';
      list.appendChild(row);
      if (index === 3) {
        var divider = document.createElement('div');
        divider.className = 'odivider';
        divider.textContent = 'beneath More…';
        list.appendChild(divider);
      }
    });
  }

  // —— Prompts ——
  function applyPrompts(payload) {
    var menu = document.getElementById('pidMenu');
    if (menu) {
      menu.innerHTML = '';
      (payload.profiles || []).forEach(function (p) {
        var item = document.createElement('div');
        item.className = 'pitem' + (p.id === payload.selectedId ? ' on' : '');
        item.dataset.v = p.id;
        item.textContent = p.id;
        item.addEventListener('click', function () {
          menu.hidden = true;
          post({ mcmd: 'select_prompt', id: p.id });
        });
        menu.appendChild(item);
      });
    }
    set('pid', payload.selectedId || '');
    set('pidLabel', payload.selectedId || '');
    var sel = payload.selected || {};
    set('pname', sel.name || '');
    set('pmodel', sel.model || '');
    set('ptemp', sel.temp == null ? '' : sel.temp);
    set('ptokens', sel.tokens == null ? '' : sel.tokens);
    set('sysprompt', sel.sys || '');
    set('usertpl', sel.user || '');
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
  function applyHistory(payload) {
    var stat = document.querySelector('#page-history .statnote');
    if (stat) stat.textContent = (payload.total || 0) + ' entries';
    var list = document.querySelector('#page-history .hlist');
    if (!list) return;
    list.innerHTML = '';
    (payload.entries || []).forEach(function (entry, index) {
      var item = document.createElement('div');
      item.className = 'hitem' + (index === payload.selected ? ' on' : '');
      item.innerHTML =
        '<div class="htop"><span class="ht">' + esc(entry.target) + '</span>' +
        '<span class="htime">' + esc(entry.time) + '</span></div>' +
        '<div class="hp">' + esc(entry.preview) + '</div>' +
        '<div class="hmeta"><span class="hprof">' + esc(entry.prompt) + '</span>' +
        '<span class="hsrc">' + esc(entry.source) + '</span></div>';
      item.addEventListener('click', function () {
        list.querySelectorAll('.hitem').forEach(function (el) { el.classList.remove('on'); });
        item.classList.add('on');
        post({ mcmd: 'select_history', index: index });
      });
      list.appendChild(item);
    });
  }

  function renderOutput(markdown) {
    var out = document.querySelector('#page-history .houtput');
    if (!out) return;
    out.innerHTML = '';
    var current = null;
    var sections = [];
    (markdown || '').split(/\r?\n/).forEach(function (line) {
      var t = line.trim();
      if (!t.length) return;
      if (line.indexOf('## ') === 0) {
        current = { title: line.slice(3).trim(), items: [], prose: [] };
        sections.push(current);
      } else if (current) {
        if (t.charAt(0) === '-' || t.charAt(0) === '*') { current.items.push(t.slice(1).trim()); }
        else { current.prose.push(t); }
      } else {
        current = { title: null, items: [], prose: [t] };
        sections.push(current);
      }
    });
    sections.forEach(function (sec) {
      var div = document.createElement('div');
      div.className = 'sec';
      var html = '';
      if (sec.title) html += '<h3>' + esc(sec.title) + '</h3>';
      if (sec.items.length) {
        html += '<ul>' + sec.items.map(function (i) { return '<li class="cline">' + esc(i) + '</li>'; }).join('') + '</ul>';
      }
      sec.prose.forEach(function (p) { html += '<p class="fineprint">' + esc(p) + '</p>'; });
      div.innerHTML = html;
      out.appendChild(div);
    });
  }

  function applyHistoryDetail(payload) {
    var entry = payload.entry || {};
    var head = document.querySelector('#page-history .hhead');
    if (head) {
      head.innerHTML = '<span class="hprof">' + esc(entry.prompt) + '</span>' +
        '<span class="htime">' + esc(entry.time + ' · ' + entry.source) + '</span>';
    }
    var sp = document.querySelector('#page-history .sourcepanel');
    if (sp) {
      var t = sp.querySelector('.target');
      var c = sp.querySelector('.context');
      if (t) t.textContent = entry.target || '';
      if (c) { c.textContent = entry.context || ''; c.style.display = entry.context ? '' : 'none'; }
    }
    renderOutput(entry.output);
  }

  window.LINGO_MANAGER = {
    applyManager: function (json) {
      var data;
      try {
        data = (typeof json === 'string') ? JSON.parse(json) : json;
      } catch (e) { return; }
      var payload = data.apply || data;
      switch (payload.kind) {
        case 'settings': applySettings(payload.config || {}); break;
        case 'prompts': applyPrompts(payload); break;
        case 'history': applyHistory(payload); break;
        case 'history_detail': applyHistoryDetail(payload); break;
        default: break;
      }
    },
    savePrompt: postPrompt
  };

  function wire() {
    // Settings: editable fields post auto-save payloads.
    ['endpoint', 'model', 'credtarget', 'hkSelection', 'hkHover', 'hkPause', 'hkPopup']
      .forEach(function (id) {
        var el = document.getElementById(id);
        if (el) el.addEventListener('change', postSettings);
      });
    // Profile order: post after any reorder (buttons or drag).
    var list = document.getElementById('orderlist');
    if (list) {
      list.addEventListener('click', function (e) {
        if (e.target.closest('button.iconbtn')) postSettings();
      });
      list.addEventListener('drop', postSettings);
    }
    var newPrompt = document.getElementById('newPromptBtn');
    if (newPrompt) newPrompt.addEventListener('click', function () {
      post({ mcmd: 'new_prompt' });
    });
    var savePrompt = document.getElementById('savePromptBtn');
    if (savePrompt) savePrompt.addEventListener('click', postPrompt);
    var refresh = document.querySelector('#page-history .htoolbar .btn');
    if (refresh) refresh.addEventListener('click', function () {
      post({ mcmd: 'refresh_history' });
    });
    var search = document.querySelector('#page-history .hsearch input');
    if (search) {
      var timer = null;
      search.addEventListener('input', function () {
        clearTimeout(timer);
        timer = setTimeout(function () {
          post({ mcmd: 'history_query', search: search.value });
        }, 400);
      });
    }
  }
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', wire);
  } else {
    wire();
  }
})();
</script>"#;
