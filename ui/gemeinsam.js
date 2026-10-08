// Gemeinsames der eigenen Seiten. Befehle gehen an die App (Capability „lokal“).
const befehl = (name, args) => window.__TAURI__.core.invoke(name, args);
const $ = (id) => document.getElementById(id);
const zeigen = (id, an) => $(id).classList.toggle('versteckt', !an);
function melden(id, text, art = 'fehler') {
  const el = $(id);
  el.className = `hinweis ${art}`;
  el.textContent = text || '';
  zeigen(id, !!text);
}
const ARTEN = { kiosk: 'Self-Service-Kiosk', anzeige: 'Verfügbarkeitsanzeige', bedient: 'Theke' };
const TYPEN = { finger: 'Fingerabdruckscanner', magnetkarte: 'Magnetkartenleser', unterschrift: 'Unterschriftenpad', qr: 'QR-Scanner', drucker: 'Bondrucker' };
const STATUS = { bereit: 'bereit', treiber_fehlt: 'Treiber fehlt', fehler: 'Fehler', erkannt: 'erkannt – folgt' };
function geraeteListe(el, usb) {
  el.innerHTML = '';
  if (!usb.length) { el.innerHTML = '<div class="leise">Keine USB-Geräte erkannt.</div>'; return; }
  for (const g of usb) {
    const d = document.createElement('div');
    d.className = 'geraet';
    d.innerHTML = `<span><b></b> <span class="leise"></span></span><span class="marke ${g.status}"></span>`;
    d.querySelector('b').textContent = g.modell || TYPEN[g.typ] || g.typ;
    d.querySelector('.leise').textContent = TYPEN[g.typ] || '';
    d.querySelector('.marke').textContent = STATUS[g.status] || g.status;
    el.appendChild(d);
  }
}
