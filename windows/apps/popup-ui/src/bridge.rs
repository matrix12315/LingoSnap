//! App-mode page glue for the fair-copy companion: hides the mock sheets'
//! caption chrome and bridges protocol events/commands to the page DOM.

/// The mock sheets are demo pages; in the product the card itself is the
/// surface, so the caption row and demo body padding are hidden while
/// everything inside `.card` / `.railwrap` stays exactly as designed.
pub const APP_CSS: &str = r#"
/* The window is sized to the page's content and has no surface of its own:
   everything outside the card and rail is see-through desktop. A small
   margin keeps room for a compact version of the mockup's lift shadow. */
html, body { background: transparent !important; width: max-content !important;
  height: max-content !important; }
body { display: flex !important; flex-direction: column !important;
  align-items: flex-start !important; justify-content: flex-start !important;
  gap: 8px !important; padding: 6px 8px 10px !important; margin: 0 !important; }
body, .rail, .railwrap, .chrome, .foot { user-select: none !important; -webkit-user-select: none !important; }
.manuscript, .sourcepanel { user-select: text !important; -webkit-user-select: text !important; }
/* The mockup's narrow-viewport rule (max-width: 900px → .pop 100%) fires
   inside this window, which is sized FROM the page: the card then grew to
   its longest unwrapped line. Hold the sheet's designed 448px measure. */
.pop { width: 448px !important; }
.rail, .rmenu, .card { box-shadow: 0 1px 1px rgba(15, 12, 8, .20), 0 3px 8px rgba(15, 12, 8, .22) !important; }
[data-theme="dark"] .rail, [data-theme="dark"] .rmenu, [data-theme="dark"] .card {
  box-shadow: 0 1px 1px rgba(0, 0, 0, .42), 0 3px 8px rgba(0, 0, 0, .46) !important; }
/* The dark pill means hover/press only; no pill is pre-selected. */
.rpill:hover, .rpill:active { color: var(--btn-primary-ink) !important; font-weight: 600 !important;
  background: var(--btn-primary-bg) !important; }
.card {
  /* display must stay overridable: the chooser hides the card inline
     (an !important stylesheet rule would beat style.display='none').
     No viewport-relative max-height: the window is sized FROM the page, so
     a vh limit would pin a card to the previous (rail-sized) window. The
     mockup's manuscript max-height bounds the card; the host clamps the
     window to the monitor work area. */
  display: flex; flex-direction: column !important; }
.manuscript { overflow-y: auto !important; min-height: 0 !important; flex: 1 1 auto !important;
  /* The glass actions float over the manuscript's lower edge; the last line
     must still be readable above them once scrolled (or on a short card). */
  padding-bottom: 62px !important; }
.caprow { display: none !important; }
.themebtn { display: none !important; }
.card .errbox[hidden], .card .manuscript[hidden], .card .foot[hidden] { display: none !important; }
"#;

/// Bridge between protocol events and the page. `window.LINGO.apply(json)`
/// is invoked per event by the host; commands are posted back through
/// WebView2 web messages as `{"cmd": ...}` JSON.
///
/// The page is the mockup itself; the bridge only fills it. Each state maps
/// onto the matching mockup sheet: `popup.html` (result), the streaming sheet
/// (status "writing…", ink caret, Copy disabled), the error sheet (grey seal,
/// "idle", in-flow quotation and the `.errbox`), and the chooser rail.
pub const BRIDGE_JS: &str = r##"<script>
(function () {
  if (window.LINGO) return;
  function post(msg) {
    try { window.chrome.webview.postMessage(JSON.stringify(msg)); } catch (e) {}
  }
  // The window is sized to the body (max-content plus the shadow margin).
  // Re-report after fonts and layout settle: one measurement races the final
  // render and clips the card's bottom edge.
  function reportHeight() {
    var r = document.body.getBoundingClientRect();
    var w = Math.ceil(r.width), h = Math.ceil(r.height);
    if (w > 0 && h > 0) post({ cmd: 'content_size', w: w, h: h });
  }
  function reportHeightSettled() {
    reportHeight();
    requestAnimationFrame(reportHeight);
    setTimeout(reportHeight, 120);
    setTimeout(reportHeight, 400);
    if (document.fonts && document.fonts.ready) {
      document.fonts.ready.then(reportHeight);
    }
  }
  // Drag by the chrome header: press, move >4px, and the host takes over
  // with the native caption loop.
  var dragState = null;
  function dragMove(e) {
    if (!dragState || dragState.sent) return;
    if (Math.abs(e.screenX - dragState.x) + Math.abs(e.screenY - dragState.y) > 4) {
      dragState.sent = true;
      post({ cmd: 'begin_drag' });
      dragUp();
    }
  }
  function dragUp() {
    dragState = null;
    window.removeEventListener('mousemove', dragMove);
    window.removeEventListener('mouseup', dragUp);
  }
  function wireDrag() {
    var chrome = document.querySelector('.card .chrome');
    if (!chrome) return;
    chrome.addEventListener('mousedown', function (e) {
      if (e.button !== 0 || e.target.closest('button')) return;
      dragState = { x: e.screenX, y: e.screenY, sent: false };
      window.addEventListener('mousemove', dragMove);
      window.addEventListener('mouseup', dragUp);
    });
  }
  function esc(text) {
    var d = document.createElement('div');
    d.textContent = text == null ? '' : String(text);
    return d.innerHTML;
  }
  // Inline Markdown: `code` reads as the mockup's .alt mono run.
  function inline(text) {
    var out = '', parts = String(text).split('`');
    for (var i = 0; i < parts.length; i++) {
      if (i % 2 === 1 && i < parts.length - 1) {
        out += '<span class="alt">' + esc(parts[i]) + '</span>';
        continue;
      }
      var seg = (i % 2 === 1) ? '`' + parts[i] : parts[i];
      seg = esc(seg)
        .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
        .replace(/__([^_]+)__/g, '<strong>$1</strong>')
        .replace(/(^|[^*\w])\*([^*\s][^*]*)\*(?!\*)/g, '$1<em>$2</em>');
      out += seg;
    }
    return out;
  }
  // Markdown → the manuscript's section grammar: every heading opens a
  // .sec with an h3; lists become the bulleted ul; prose of a heading-less
  // first section is the hero translation line (.trans), other prose is the
  // soft fine print, exactly as the mockup sheets set them.
  function renderMarkdown(markdown) {
    var lines = String(markdown || '').split(/\r?\n/);
    var sections = [], cur = null, list = null, para = [], fence = null;
    function open(title) { cur = { title: title, blocks: [] }; sections.push(cur); list = null; }
    function flushPara() {
      if (!para.length) return;
      if (!cur) open(null);
      cur.blocks.push({ kind: 'p', text: para.join(' ') });
      para = [];
    }
    lines.forEach(function (line) {
      if (fence) {
        if (/^\s*```/.test(line)) { cur.blocks.push({ kind: 'code', text: fence.join('\n') }); fence = null; }
        else fence.push(line);
        return;
      }
      if (/^\s*```/.test(line)) { flushPara(); if (!cur) open(null); fence = []; list = null; return; }
      var t = line.trim();
      if (!t.length) { flushPara(); list = null; return; }
      var h = t.match(/^#{1,6}\s+(.*)$/);
      if (h) { flushPara(); open(h[1].replace(/[:：]\s*$/, '')); return; }
      var li = t.match(/^(?:[-*+•]|\d+[.)])\s+(.*)$/);
      if (li) {
        flushPara();
        if (!cur) open(null);
        if (!list) { list = { kind: 'ul', items: [] }; cur.blocks.push(list); }
        list.items.push(li[1]);
        return;
      }
      if (/^>\s?/.test(t)) {
        flushPara();
        if (!cur) open(null);
        cur.blocks.push({ kind: 'quote', text: t.replace(/^>\s?/, '') });
        list = null;
        return;
      }
      if (/^\|.*\|$/.test(t)) {
        if (/^\|[\s:|-]+\|$/.test(t)) return;
        flushPara();
        if (!cur) open(null);
        var cells = t.slice(1, -1).split('|').map(function (c) { return c.trim(); });
        var last = cur.blocks[cur.blocks.length - 1];
        if (!last || last.kind !== 'table') { last = { kind: 'table', rows: [] }; cur.blocks.push(last); }
        last.rows.push(cells);
        list = null;
        return;
      }
      list = null;
      para.push(t);
    });
    if (fence) { cur.blocks.push({ kind: 'code', text: fence.join('\n') }); }
    flushPara();
    var html = '';
    sections.forEach(function (sec, index) {
      html += '<div class="sec">';
      if (sec.title) html += '<h3>' + inline(sec.title) + '</h3>';
      sec.blocks.forEach(function (b) {
        if (b.kind === 'ul') {
          html += '<ul>' + b.items.map(function (i) { return '<li>' + inline(i) + '</li>'; }).join('') + '</ul>';
        } else if (b.kind === 'p') {
          var hero = index === 0;
          html += '<p class="' + (hero ? 'trans' : 'fineprint') + '">' + inline(b.text) + '</p>';
        } else if (b.kind === 'quote') {
          html += '<blockquote>' + inline(b.text) + '</blockquote>';
        } else if (b.kind === 'code') {
          html += '<p class="fineprint"><span class="alt" style="white-space:pre-wrap">' + esc(b.text) + '</span></p>';
        } else if (b.kind === 'table') {
          html += '<table>' + b.rows.map(function (row, r) {
            var tag = r === 0 ? 'th' : 'td';
            return '<tr>' + row.map(function (c) { return '<' + tag + '>' + inline(c) + '</' + tag + '>'; }).join('') + '</tr>';
          }).join('') + '</table>';
        }
      });
      html += '</div>';
    });
    return html;
  }
  function q(sel) { return document.querySelector(sel); }
  function manuscript() { return q('.card .manuscript'); }
  var card = q('.card');
  var sourcepanel = q('.card .sourcepanel');
  var foot = q('.card .foot');
  var seal = q('.card .chrome .seal');
  var status = q('.card .chrome .who span');
  var copyBtn = document.getElementById('copyBtn');
  var STATUS_READY = status ? status.innerHTML : '';
  var errbox = null;
  function refit() {
    var m = manuscript();
    if (sourcepanel && m && sourcepanel.classList.contains('float')) {
      m.style.paddingTop = (sourcepanel.offsetTop + sourcepanel.offsetHeight - m.offsetTop + 24) + 'px';
    }
  }
  function setSource(target, context) {
    if (!sourcepanel) return;
    var t = sourcepanel.querySelector('.target');
    var c = sourcepanel.querySelector('.context');
    if (t) t.textContent = target || '';
    if (c) { c.textContent = context || ''; c.style.display = context ? '' : 'none'; }
  }
  // Card states — result / streaming / error — as the three mockup sheets.
  function setMode(mode) {
    var m = manuscript();
    var error = mode === 'error';
    if (m) m.hidden = error;
    if (foot) foot.hidden = error;
    if (errbox) errbox.hidden = !error;
    if (sourcepanel) sourcepanel.classList.toggle('float', !error);
    if (seal) seal.style.background = error ? '#8A8375' : '';
    if (status) {
      if (mode === 'streaming') status.innerHTML = '<i class="live-dot"></i>writing\u2026';
      else if (error) status.textContent = 'idle';
      else status.innerHTML = STATUS_READY;
    }
    if (copyBtn) {
      copyBtn.disabled = mode !== 'result';
      copyBtn.style.opacity = mode === 'result' ? '' : '.55';
    }
    refit();
  }
  function showError(message) {
    if (!errbox && card) {
      errbox = document.createElement('div');
      errbox.className = 'errbox';
      errbox.innerHTML = '<div class="headline"></div><div class="detail"></div>' +
        '<div class="actions"><button class="btn sm" type="button" data-act="retry">Retry</button>' +
        '<button class="btn sm" type="button" data-act="close">Close</button></div>';
      errbox.addEventListener('click', function (e) {
        var b = e.target.closest('button');
        if (!b) return;
        if (b.dataset.act === 'retry') window.LINGO.retry();
        else window.LINGO.close();
      });
      if (sourcepanel && sourcepanel.nextSibling) card.insertBefore(errbox, sourcepanel.nextSibling);
      else card.appendChild(errbox);
      lens(errbox);
    }
    var text = String(message || 'Something went wrong.');
    var cut = text.search(/[.!?。！？]\s/);
    var headline = cut > 0 ? text.slice(0, cut + 1) : text;
    var detail = cut > 0 ? text.slice(cut + 1).trim() : '';
    errbox.querySelector('.headline').textContent = headline;
    var d = errbox.querySelector('.detail');
    d.textContent = detail;
    d.style.display = detail ? '' : 'none';
    // Without a quotation above it the chrome's rule already separates it.
    var quoted = sourcepanel && sourcepanel.style.display !== 'none';
    errbox.style.borderTop = quoted ? '' : 'none';
    setMode('error');
  }
  function lens(root) {
    if (window.liquidLens) {
      requestAnimationFrame(function () { window.liquidLens(root); });
    }
  }
  // Rebuilt controls must take their lens filters with them, or every
  // chooser would leave its SVG displacement maps behind in the page.
  function dropLens(root) {
    if (!root) return;
    root.querySelectorAll('[data-lens]').forEach(function (el) {
      var f = document.getElementById('ll-' + el.dataset.lens);
      if (f && f.parentNode) f.parentNode.remove();
    });
  }
  var streamText = '';
  var lastOutput = '';
  function setChooser(visible, keepCard) {
    var wrap = document.getElementById('chooser');
    if (wrap) wrap.style.display = visible ? 'flex' : 'none';
    if (card) card.style.display = keepCard ? '' : 'none';
  }
  function cardVisible() { return card && card.style.display !== 'none'; }
  function buildRail(p) {
    var wrap = document.getElementById('chooser');
    if (!wrap) return false;
    var rail = wrap.querySelector('.rail');
    if (!rail || !p.profiles || !p.profiles.length) return false;
    var old = wrap.querySelector('.rmenu');
    dropLens(wrap);
    if (old) old.remove();
    rail.innerHTML = '';
    // The classic chooser: four compact pills inline, the rest folded under
    // a More… capsule; clicks post the TRUE profile index.
    var LIMIT = 4;
    function compact(name) {
      var word = String(name).trim().split(/\s+/).filter(function (part) { return part.length; })[0] || 'Profile';
      return word.length > 11 ? word.slice(0, 11) + '\u2026' : word;
    }
    function pill(label, index, role) {
      var b = document.createElement('button');
      b.type = 'button';
      b.className = 'rpill';
      if (role) b.setAttribute('role', role);
      b.textContent = label;
      b.addEventListener('click', function () {
        b.classList.add('on');
        post({ cmd: 'select_profile', index: index });
      });
      return b;
    }
    for (var i = 0; i < Math.min(LIMIT, p.profiles.length); i++) {
      rail.appendChild(pill(compact(p.profiles[i]), i));
    }
    if (p.profiles.length > LIMIT) {
      var rdiv = document.createElement('span');
      rdiv.className = 'rdiv';
      rdiv.setAttribute('aria-hidden', 'true');
      rail.appendChild(rdiv);
      var more = document.createElement('button');
      more.type = 'button';
      more.className = 'rpill rmore';
      more.setAttribute('aria-haspopup', 'menu');
      more.innerHTML = 'More\u2026 <svg width="9" height="9" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"><path d="M2 3.5l3 3 3-3"/></svg>';
      var menu = document.createElement('div');
      menu.className = 'rmenu';
      menu.style.display = 'none';
      for (var j = LIMIT; j < p.profiles.length; j++) {
        menu.appendChild(pill(p.profiles[j], j, 'menuitem'));
      }
      more.addEventListener('click', function (e) {
        e.stopPropagation();
        var opening = menu.style.display === 'none';
        menu.style.display = opening ? '' : 'none';
        if (opening) lens(menu);
        // The window is sized to the content; an open menu needs room.
        reportHeightSettled();
      });
      rail.appendChild(more);
      wrap.appendChild(menu);
    }
    return true;
  }
  window.LINGO = {
    apply: function (json) {
      var ev;
      try { ev = (typeof json === 'string') ? JSON.parse(json) : json; } catch (e) { return; }
      var p = ev.payload || ev;
      switch (p.type) {
        case 'show_loading': {
          setChooser(false, true);
          streamText = '';
          lastOutput = '';
          setSource(p.target || '', p.context || '');
          var m = manuscript();
          if (m) { m.innerHTML = '<div class="sec"><div class="trans"><span class="caret" aria-hidden="true"></span></div></div>'; m.scrollTop = 0; }
          setMode('streaming');
          break;
        }
        case 'delta': {
          streamText += p.text || '';
          var m = manuscript();
          if (m) {
            m.innerHTML = renderMarkdown(streamText);
            // The vermilion ink caret sits at the insertion point.
            var blocks = m.querySelectorAll('.sec > *:not(h3)');
            var last = blocks.length ? blocks[blocks.length - 1] : null;
            if (last && last.tagName === 'UL' && last.lastElementChild) last = last.lastElementChild;
            var caret = '<span class="caret" aria-hidden="true"></span>';
            if (last) last.insertAdjacentHTML('beforeend', caret);
            else m.innerHTML = '<div class="sec"><div class="trans">' + caret + '</div></div>';
          }
          setMode('streaming');
          break;
        }
        case 'completed': {
          lastOutput = p.text || streamText;
          var m = manuscript();
          if (m) m.innerHTML = renderMarkdown(lastOutput);
          setMode('result');
          break;
        }
        case 'local_error':
          if (p.anchor) {
            // An error with nothing presented before it: no stale quote.
            setChooser(false, true);
            setSource('', '');
            if (sourcepanel) sourcepanel.style.display = 'none';
          }
          showError(p.message);
          break;
        case 'set_pinned': {
          var pin = document.getElementById('pinBtn');
          if (pin) pin.setAttribute('aria-pressed', String(!!p.pinned));
          break;
        }
        case 'show_chooser':
          if (!buildRail(p)) break;
          // In-card selection keeps the card being read under the rail; a
          // fresh selection shows the rail alone.
          setChooser(true, !!p.keeps_card && cardVisible());
          lens(document.getElementById('chooser'));
          break;
        case 'close': {
          // The host hides the window; the rail must not linger in the page
          // and re-report its size.
          var wrap = document.getElementById('chooser');
          if (wrap) wrap.style.display = 'none';
          break;
        }
        default:
          break;
      }
      if (p.type === 'show_loading' && sourcepanel) {
        sourcepanel.style.display = '';
        // The quotation's glass lens is sized to its pixels; a new quote
        // has a new size.
        if (window.liquidLens) {
          requestAnimationFrame(function () { window.liquidLens.refresh(sourcepanel); refit(); });
        }
      }
      reportHeightSettled();
    },
    copy: function () {
      var m = manuscript();
      var text = lastOutput || (m ? m.textContent.trim() : '');
      if (text) post({ cmd: 'copy', text: text });
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
    if (copyBtn) copyBtn.addEventListener('click', function () { window.LINGO.copy(); });
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
    // Esc closes the surface, like the classic popup.
    document.addEventListener('keydown', function (e) {
      if (e.key === 'Escape') window.LINGO.close();
    });
  }
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', wire);
  } else {
    wire();
  }
  wireDrag();
  // Initial size report: ignored by the host until a presentation event
  // arrives (dev previews use it to size the sample sheet).
  reportHeightSettled();
  // Recursive self-translation, matching the classic popup: selecting text
  // inside the RESULT (and only there) raises the chooser for the selection.
  document.addEventListener('mouseup', function (e) {
    var m = manuscript();
    if (!m || !m.contains(e.target)) return;
    var sel = window.getSelection();
    if (!sel || !sel.rangeCount) return;
    if (!m.contains(sel.getRangeAt(0).commonAncestorContainer)) return;
    var text = String(sel).trim();
    if (text.length) post({ cmd: 'self_translate', text: text });
  });
})();
</script>"##;
