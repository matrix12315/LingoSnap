// Liquid glass — faithful port of liquid-glass-studio's fragment-main.glsl
// (final pass, STEP <= 9):
//   rounded-rect SDF → outward normal → refraction band of `band` px at the
//   boundary with the shader's quadratic profile:
//     x_R_ratio = 1 - d/band;  thetaI = asin(x_R_ratio^2);
//     thetaT = asin(sin(thetaI)/refFactor);  edgeFactor = -tan(thetaT - thetaI)
//   sample pulled INWARD along the normal (uv - normal*edgeFactor*refDistance),
//   per-channel dispersion exactly as getTextureDispersion (N_R .98 / N_G 1 /
//   N_B 1.02 × dispersion), background kept SHARP (demo blurRadius = 1).
// One filter per button, sized to its own pixels; fresnel/glare live in CSS.
// Parameters: exported studio preset liquid-glass-2026-10-06T09-20-02.json
// (refThickness 18.77, refDistance 0.029 scene → 26px at a 900px reference
// height, refFactor 1.4, refDispersion 7, blurRadius 1, tint transparent).
(() => {
  const REF = { band: 18.77, refDistance: 26.1, refFactor: 1.4, dispersion: 7 };

  const roundedRectSDF = (w, h, r) => (x, y) => {
    const qx = Math.abs(x - w / 2) - (w / 2 - r);
    const qy = Math.abs(y - h / 2) - (h / 2 - r);
    const ox = Math.max(qx, 0), oy = Math.max(qy, 0);
    return Math.hypot(ox, oy) + Math.min(Math.max(qx, qy), 0) - r;
  };

  const buildFilter = (el, id) => {
    const rect = el.getBoundingClientRect();
    const w = Math.max(8, Math.ceil(rect.width));
    const h = Math.max(8, Math.ceil(rect.height));
    const cs = getComputedStyle(el);
    let r = parseFloat(cs.borderRadius) || 0;
    r = Math.min(r, w / 2, h / 2);
    const band = Math.min(REF.band, h * 0.9);
    const refDist = REF.refDistance;
    const sd = roundedRectSDF(w, h, r);

    const c = document.createElement('canvas');
    c.width = w; c.height = h;
    const ctx = c.getContext('2d');
    const img = ctx.createImageData(w, h);
    const off = new Float32Array(w * h * 2);
    let maxAbs = 0.001;

    for (let y = 0; y < h; y++) {
      for (let x = 0; x < w; x++) {
        const d = sd(x + 0.5, y + 0.5);          // negative inside
        const nmerged = -d;
        let ox = 0, oy = 0;
        if (nmerged > 0 && nmerged < band) {
          const t = Math.max(0, 1 - nmerged / band);
          const thetaI = Math.asin(Math.min(1, t * t));
          const thetaT = Math.asin(Math.min(1, Math.sin(thetaI) / REF.refFactor));
          const edgeFactor = Math.max(0, -Math.tan(thetaT - thetaI));
          const e = 1;
          let gx = sd(x + 0.5 + e, y + 0.5) - sd(x + 0.5 - e, y + 0.5);
          let gy = sd(x + 0.5, y + 0.5 + e) - sd(x + 0.5, y + 0.5 - e);
          const gl = Math.hypot(gx, gy) || 1;
          gx /= gl; gy /= gl;
          ox = -gx * edgeFactor * refDist;       // inward along the normal
          oy = -gy * edgeFactor * refDist;
        }
        const k = (y * w + x) * 2;
        off[k] = ox; off[k + 1] = oy;
        maxAbs = Math.max(maxAbs, Math.abs(ox), Math.abs(oy));
      }
    }

    for (let p = 0; p < w * h; p++) {
      const i = p * 4;
      img.data[i]     = Math.round(128 + (off[p * 2]     / (2 * maxAbs)) * 255);
      img.data[i + 1] = Math.round(128 + (off[p * 2 + 1] / (2 * maxAbs)) * 255);
      img.data[i + 2] = 128;
      img.data[i + 3] = 255;
    }
    ctx.putImageData(img, 0, 0);
    const mapUrl = c.toDataURL();

    const S = 2 * maxAbs;                        // ±maxAbs px encoded over 0..255
    const S_R = S * (1 + 0.02 * REF.dispersion); // N_R = 0.98 → red bends more
    const S_B = S * (1 - 0.02 * REF.dispersion); // N_B = 1.02 → blue bends less
    const iso = (ch) => {
      const v = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0];
      v[ch] = 1;
      return 'values="' + v.join(' ') + '"';
    };

    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('width', '0');
    svg.setAttribute('height', '0');
    svg.setAttribute('style', 'position:absolute');
    svg.setAttribute('aria-hidden', 'true');
    svg.innerHTML =
      '<filter id="ll-' + id + '" x="0%" y="0%" width="100%" height="100%" color-interpolation-filters="sRGB">' +
      '<feImage href="' + mapUrl + '" result="map" preserveAspectRatio="none"></feImage>' +
      '<feGaussianBlur in="SourceGraphic" stdDeviation="1" result="soft"></feGaussianBlur>' +
      '<feDisplacementMap in="soft" in2="map" scale="' + S_R + '" xChannelSelector="R" yChannelSelector="G" result="dR"></feDisplacementMap>' +
      '<feDisplacementMap in="soft" in2="map" scale="' + S + '" xChannelSelector="R" yChannelSelector="G" result="dG"></feDisplacementMap>' +
      '<feDisplacementMap in="soft" in2="map" scale="' + S_B + '" xChannelSelector="R" yChannelSelector="G" result="dB"></feDisplacementMap>' +
      '<feColorMatrix in="dR" ' + iso(0) + ' result="cR"></feColorMatrix>' +
      '<feColorMatrix in="dG" ' + iso(5) + ' result="cG"></feColorMatrix>' +
      '<feColorMatrix in="dB" ' + iso(10) + ' result="cB"></feColorMatrix>' +
      '<feBlend in="cR" in2="cG" mode="screen" result="rg"></feBlend>' +
      '<feBlend in="rg" in2="cB" mode="screen"></feBlend>' +
      '</filter>';
    document.body.appendChild(svg);

    el.style.webkitBackdropFilter = 'url(#ll-' + id + ')';
    el.style.backdropFilter = 'url(#ll-' + id + ')';
  };

  document.querySelectorAll('.btn, .rpill, .mnav button, .iconbtn')
    .forEach((el, i) => buildFilter(el, i));
})();
