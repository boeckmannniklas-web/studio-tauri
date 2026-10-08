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
    // Fehlt etwas: was zu tun ist – und die Download-Seite des Herstellers als Rückfall
    if (g.status !== 'bereit' && (g.meldung || g.link)) {
      const hilfe = document.createElement('div');
      hilfe.className = 'leise';
      hilfe.style.margin = '-4px 0 8px';
      hilfe.textContent = g.meldung || '';
      if (g.link) {
        const a = document.createElement('a');
        a.href = '#';
        a.textContent = ' Download-Seite öffnen';
        a.onclick = (e) => { e.preventDefault(); befehl('seite_oeffnen', { url: g.link }).catch((x) => alert(String(x))); };
        hilfe.appendChild(a);
      }
      el.appendChild(hilfe);
    }
  }
}

// Anleitung zum Fingerabdruckscanner – auf der Einrichtungs- und der Wartungsseite gleich
const ANLEITUNG_FINGER = `
<summary>Anleitung: Fingerabdruckscanner einrichten</summary>
<ol>
  <li>Scanner direkt an einen USB-Anschluss dieses PCs stecken (nicht über einen Hub). Er erscheint
    oben unter „Geräte an diesem PC“.</li>
  <li><b>„Geräte einrichten“</b> tippen. Die App holt Treiber und SecuGen-Bibliothek vom Studio-Server.</li>
  <li>Windows fragt <b>einmal</b>, ob die App Änderungen vornehmen darf → <b>Ja</b>. Ohne
    Administratorkonto: Anmeldung eines Administrators eingeben.</li>
  <li>Nach wenigen Sekunden steht beim Scanner <b>„bereit“</b>. Ein Neustart ist nicht nötig.</li>
</ol>
<p><b>Unterstützte Scanner</b> (SecuGen, Windows 10/11 64 Bit): Hamster Plus (SDU03M, SDU03P, FDU03) ·
  Hamster IV (FDU04A, SDU04P) · Hamster Pro 20 (U20) · Hamster Pro und Pro Duo (UPx, UPx-P).</p>
<p><b>Klappt es nicht?</b></p>
<ul>
  <li>„Kein Gerätepaket auf diesem Server“: Der Studio-Server ist zu alt – erst ihn aktualisieren.</li>
  <li>Scanner steht weiter auf „Treiber fehlt“: App im Wartungsmenü beenden und neu starten.</li>
  <li>Scanner taucht nicht auf: anderen USB-Anschluss oder anderes Kabel nehmen.</li>
  <li>Von Hand (Eingabeaufforderung als Administrator): Treiber mit
    <code>pnputil /add-driver "&lt;Ordner&gt;\\SGFu03x64.inf" /install</code> (SDU03M; die anderen
    Scanner haben eigene INF-Dateien), dann die DLLs aus dem SecuGen <i>FDx SDK Pro for Windows</i>,
    Ordner <code>bin\\x64</code>, nach <code>C:\\ProgramData\\StudioKassenplatz\\geraete\\secugen</code> kopieren.</li>
</ul>`;
function anleitungFinger(el) { el.innerHTML = ANLEITUNG_FINGER; }
