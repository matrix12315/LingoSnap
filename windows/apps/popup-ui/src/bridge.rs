//! App-mode page glue for the fair-copy companion: hides the mock sheets'
//! caption chrome and bridges protocol events/commands to the page DOM.

/// The mock sheets are demo pages; in the product the card itself is the
/// surface, so the caption row and demo body padding are hidden while
/// everything inside `.card` / `.railwrap` stays exactly as designed.
pub const APP_CSS: &str = r#"
html, body { background: transparent !important; width: max-content !important;
  height: max-content !important; align-items: flex-start !important;
  justify-content: flex-start !important; }
body, .rail, .railwrap, .chrome, .foot { user-select: none !important; -webkit-user-select: none !important; }
.manuscript, .sourcepanel { user-select: text !important; -webkit-user-select: text !important; }
.rail, .card { box-shadow: none !important; }
.card { width: 460px !important; max-height: calc(100vh - 20px) !important;
  display: flex !important; flex-direction: column !important; }
.manuscript { overflow-y: auto !important; min-height: 0 !important; flex: 1 1 auto !important; }
body { display: flex !important; flex-direction: column; align-items: center;
       justify-content: center; gap: 0 !important; padding: 0 !important; }
.caprow { display: none !important; }
.themebtn { display: none !important; }
"#;

/// Bridge between protocol events and the page. `window.LINGO.apply(json)`
/// is invoked per event by the host; commands are posted back through
/// WebView2 web messages as `{"cmd": ...}` JSON.
pub const BRIDGE_JS: &str = r#"<script>
(function () {
  if (window.LINGO) return;
  function post(msg) {
    try { window.chrome.webview.postMessage(JSON.stringify(msg)); } catch (e) {}
  }
  // Report the visible content height, then re-report after the fonts and
  // layout settle: a single measurement races the final render and leaves
  // the bottom of the card clipped by the window edge.
  function reportHeight() {
    var visible = document.getElementById('chooser');
    var el = (visible && visible.style.display !== 'none')
      ? visible
      : document.querySelector('.card');
    if (el) post({ cmd: 'content_size', w: el.offsetWidth, h: el.offsetHeight });
  }
  function reportHeightSettled() {
    reportHeight();
    setTimeout(reportHeight, 120);
    setTimeout(reportHeight, 400);
    if (document.fonts && document.fonts.ready) {
      document.fonts.ready.then(reportHeight);
    }
  }
  // Drag by the chrome header: press, move >4px, and the host takes over
  // with the native caption loop (which swallows the mouseup — a clean
  // press-release on a pill still counts as a click instead).
  var dragState = null;
  function dragMove(e) {
    if (!dragState || dragState.sent) return;
    if (Math.abs(e.screenX - dragState.x) + Math.abs(e.screenY - dragState.y) > 4) {
      dragState.sent = true;
      post({ cmd: 'begin_drag' });
      window.removeEventListener('mousemove', dragMove);
      window.removeEventListener('mouseup', dragUp);
      dragState = null;
    }
  }
  function dragUp() {
    dragState = null;
    window.removeEventListener('mousemove', dragMove);
    window.removeEventListener('mouseup', dragUp);
  }
  function wireDrag() {
    var beginOn = function (surface) {
      surface.addEventListener('mousedown', function (e) {
        if (e.button !== 0) return;
        if (e.target.closest('button')) return;
        dragState = { x: e.screenX, y: e.screenY, sent: false };
        window.addEventListener('mousemove', dragMove);
        window.addEventListener('mouseup', dragUp);
      });
    };
    var chrome = document.querySelector('.card .chrome');
    if (chrome) beginOn(chrome);
  }
  function esc(text) {
    var d = document.createElement('div');
    d.textContent = text == null ? '' : String(text);
    return d.innerHTML;
  }
  function manuscript() { return document.querySelector('.manuscript'); }
  function refit() {
    var sp = document.querySelector('.sourcepanel'), m = manuscript();
    if (sp && m && sp.offsetTop >= 0) {
      m.style.paddingTop = (sp.offsetTop + sp.offsetHeight - m.offsetTop + 24) + 'px';
    }
  }
  function renderManuscript(markdown) {
    var m = manuscript();
    if (!m) return;
    m.innerHTML = '';
    var sections = [];
    var current = null;
    markdown.split(/\r?\n/).forEach(function (line) {
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
        html += '<ul>' + sec.items.map(function (i) { return '<li>' + esc(i) + '</li>'; }).join('') + '</ul>';
      }
      sec.prose.forEach(function (p) { html += '<p class="fineprint">' + esc(p) + '</p>'; });
      div.innerHTML = html;
      m.appendChild(div);
    });
  }
  function setSource(target, context) {
    var sp = document.querySelector('.sourcepanel');
    if (!sp) return;
    var t = sp.querySelector('.target');
    var c = sp.querySelector('.context');
    if (t) t.innerHTML = esc(target);
    if (c) { c.innerHTML = esc(context || ''); c.style.display = context ? '' : 'none'; }
    refit();
  }
  var streamText = '';
  var lastOutput = '';
  window.LINGO = {
    apply: function (json) {
      var ev;
      try {
        // The host inlines the event as an object literal; a string payload
        // (from older callers) is parsed for compatibility.
        ev = (typeof json === 'string') ? JSON.parse(json) : json;
      } catch (e) { return; }
      var p = ev.payload || ev;
      // The chooser rail and the result card never show together: a
      // selection first presents the rail (like the classic popup's profile
      // bar), and the card only returns once a profile was picked.
      function setChooser(visible) {
        var wrap = document.getElementById('chooser');
        if (wrap) wrap.style.display = visible ? 'flex' : 'none';
        var card = document.querySelector('.card');
        if (card) card.style.display = visible ? 'none' : '';
      }
      switch (p.type) {
        case 'show_loading':
          setChooser(false);
          streamText = '';
          lastOutput = '';
          setSource(p.target || '', p.context || '');
          var m = manuscript();
          if (m) {
            m.innerHTML = '<div class="sec"><h3>Translating\u2026</h3>' +
              '<p class="fineprint">waiting for the model</p></div>';
            refit();
          }
          break;
        case 'delta': {
          streamText += p.text || '';
          var m = manuscript();
          if (m) {
            m.innerHTML = '<div class="sec"><h3>Translating\u2026</h3>' +
              '<p class="fineprint">' + esc(streamText) + '</p></div>';
          }
          break;
        }
        case 'completed':
          lastOutput = p.text || streamText;
          renderManuscript(lastOutput);
          break;
        case 'local_error': {
          var m = manuscript();
          if (m) {
            m.innerHTML = '<div class="sec"><h3>Error</h3>' +
              '<p class="fineprint">' + esc(p.message) + '</p></div>';
          }
          break;
        }
        case 'set_pinned': {
          var pin = document.getElementById('pinBtn');
          if (pin) pin.setAttribute('aria-pressed', String(!!p.pinned));
          break;
        }
        case 'show_chooser': {
          var wrap = document.getElementById('chooser');
          if (!wrap || !p.profiles || !p.profiles.length) break;
          var rail = wrap.querySelector('.rail');
          if (!rail) break;
          // Mirror the classic chooser: four compact pills inline, the rest
          // folded under a More… capsule, clicks post the TRUE profile index.
          var old = wrap.querySelector('.rmenu');
          if (old) old.remove();
          rail.innerHTML = '';
          var LIMIT = 4;
          function compact(name) {
            var word = String(name).trim().split(/\s+/).filter(function (part) { return part.length; })[0] || 'Profile';
            return word.length > 11 ? word.slice(0, 11) + '…' : word;
          }
          var inline = [];
          for (var i = 0; i < Math.min(LIMIT, p.profiles.length); i++) inline.push(i);
          var more = p.profiles.length > LIMIT;
          inline.forEach(function (index) {
            var b = document.createElement('button');
            b.type = 'button';
            b.className = 'rpill' + (p.highlight === index ? ' on' : '');
            b.textContent = compact(p.profiles[index]);
            b.addEventListener('click', function () {
              rail.querySelectorAll('.rpill').forEach(function (p) { p.classList.remove('on'); });
              b.classList.add('on');
              post({ cmd: 'select_profile', index: index });
            });
            rail.appendChild(b);
          });
          if (more) {
            var rdiv = document.createElement('span');
            rdiv.className = 'rdiv';
            rdiv.setAttribute('aria-hidden', 'true');
            rail.appendChild(rdiv);
            var m = document.createElement('button');
            m.type = 'button';
            m.className = 'rpill rmore';
            m.textContent = 'More…';
            m.setAttribute('aria-haspopup', 'menu');
            var menu = document.createElement('div');
            menu.className = 'rmenu';
            menu.style.display = 'none';
            for (var j = LIMIT; j < p.profiles.length; j++) {
              (function (index) {
                var item = document.createElement('button');
                item.type = 'button';
                item.className = 'rpill';
                item.setAttribute('role', 'menuitem');
                item.textContent = p.profiles[index];
                item.addEventListener('click', function () {
                  post({ cmd: 'select_profile', index: index });
                });
                menu.appendChild(item);
              })(j);
            }
            m.addEventListener('click', function (e) {
              e.stopPropagation();
              menu.style.display = menu.style.display === 'none' ? '' : 'none';
              // The host sizes the window to the visible content; an open
              // menu needs a taller window or it is clipped into nothing.
              reportHeightSettled();
            });
            rail.appendChild(m);
            wrap.appendChild(menu);
          }
          setChooser(true);
          break;
        }
        default:
          break;
      }
      // Report the visible content height so the host can size the window to
      // the card (or rail) instead of showing a fixed dark slab.
      var visible = document.getElementById('chooser');
      var el = (visible && visible.style.display !== 'none')
        ? visible
        : document.querySelector('.card');
      if (el) { reportHeightSettled(); } else { reportHeightSettled(); }
    },
    copy: function () {
      var line = document.getElementById('transLine');
      var m = manuscript();
      var text = lastOutput
        || (line ? line.textContent.trim() : '')
        || (m ? m.textContent.trim() : '');
      post({ cmd: 'copy', text: text });
    },
    retry: function () { post({ cmd: 'retry' }); },
    close: function () { post({ cmd: 'close' }); },
    togglePin: function () {
      var pin = document.getElementById('pinBtn');
      var on = pin && pin.getAttribute('aria-pressed') === 'true';
      post({ cmd: 'pin', pinned: !on });
    }
  };
  function wire() {
    var copy = document.getElementById('copyBtn');
    if (copy) copy.addEventListener('click', function () { window.LINGO.copy(); });
    var pin = document.getElementById('pinBtn');
    if (pin) pin.addEventListener('click', function (e) {
      e.stopImmediatePropagation();
      window.LINGO.togglePin();
    }, true);
    document.querySelectorAll('.foot .btn').forEach(function (b) {
      var label = b.textContent.trim();
      if (label === 'Retry') b.addEventListener('click', function () { window.LINGO.retry(); });
      if (label === 'Close') b.addEventListener('click', function () { window.LINGO.close(); });
    });
    document.querySelectorAll('.chrome .iconbtn').forEach(function (b) {
      if (b.getAttribute('aria-label') === 'Close') {
        b.addEventListener('click', function () { window.LINGO.close(); });
      }
    });
  }
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', wire);
  } else {
    wire();
  }
  wireDrag();
  // Recursive self-translation, matching the classic popup: selecting text
  // inside the RESULT (and only there) runs the pipeline on the selection.
  // Selections elsewhere are drag artifacts of the chrome/rail and must be
  // ignored — they once carried the whole UI text as a "translation".
  document.addEventListener('mouseup', function (e) {
    var m = document.querySelector('.manuscript');
    if (!m || !m.contains(e.target)) return;
    var sel = window.getSelection();
    if (!sel || !sel.rangeCount) return;
    if (!m.contains(sel.getRangeAt(0).commonAncestorContainer)) return;
    var text = String(sel).trim();
    if (text.length) post({ cmd: 'self_translate', text: text });
  });
})();
</script>"#;
