// Einrichtung: Studio-Server finden, Koppelcode eingeben, Geräte einrichten, starten.

let gewaehlt = '';

function formatCode(roh) {
  if (roh.startsWith('studio-kassenplatz:')) return roh; // gescannter QR-Code bleibt, wie er ist
  const c = roh.toUpperCase().replace(/[^A-Z0-9]/g, '').slice(0, 6);
  return c.length > 3 ? `${c.slice(0, 3)} ${c.slice(3)}` : c;
}

function fundeZeigen(funde) {
  const el = $('funde');
  el.innerHTML = '';
  if (!funde.length) {
    el.innerHTML = '<div class="leise">Kein Studio-Server gefunden – Adresse von Hand eingeben.</div>';
    return;
  }
  for (const f of funde) {
    const b = document.createElement('button');
    b.type = 'button';
    b.className = 'fund';
    b.innerHTML = '<span><span class="n"></span><small></small></span><span>›</span>';
    b.querySelector('.n').textContent = f.name || 'Studio-Server';
    b.querySelector('small').textContent = `${f.adresse} · Version ${f.version}`;
    b.onclick = () => {
      gewaehlt = f.adresse;
      $('edge').value = f.adresse.replace(/^http:\/\//, '');
      for (const x of el.children) x.classList.remove('an');
      b.classList.add('an');
      $('code').focus();
    };
    el.appendChild(b);
  }
  if (funde.length === 1) el.firstChild.click();
}

async function suchen() {
  $('suchen').disabled = true;
  $('funde').innerHTML = '<div class="leise">Suche im Netz …</div>';
  try { fundeZeigen(await befehl('edge_suchen')); }
  catch (e) { melden('hinweis', String(e)); }
  finally { $('suchen').disabled = false; }
}

function fertigZeigen(s) {
  zeigen('koppeln', false);
  zeigen('fertig', true);
  $('gekoppelt').textContent = `Gekoppelt als Kassenplatz ${s.platz_nr} · ${s.platz_name} (${ARTEN[s.art] || s.art}) mit ${s.edge_name || s.edge}.`;
  geraeteListe($('usb'), s.usb);
  zeigen('treiber', s.usb.some((g) => g.typ === 'finger'));
}

async function koppeln() {
  melden('hinweis', '');
  const code = $('code').value.trim();
  const edge = $('edge').value.trim() || gewaehlt;
  if (!code) { melden('hinweis', 'Bitte den Koppelcode eingeben.'); return; }
  $('los').disabled = true;
  try { fertigZeigen(await befehl('koppeln', { edge, code })); }
  catch (e) { melden('hinweis', String(e)); }
  finally { $('los').disabled = false; }
}

async function treiber() {
  $('treiber').disabled = true;
  melden('hinweis', 'Treiber werden eingerichtet …', 'warn');
  try {
    melden('hinweis', await befehl('geraete_einrichten'), 'gut');
    fertigZeigen(await befehl('einrichtung_stand'));
  } catch (e) { melden('hinweis', String(e)); }
  finally { $('treiber').disabled = false; }
}

$('code').addEventListener('input', (e) => { e.target.value = formatCode(e.target.value); });
$('code').addEventListener('keydown', (e) => { if (e.key === 'Enter') koppeln(); });
$('suchen').onclick = suchen;
$('los').onclick = koppeln;
$('treiber').onclick = treiber;
$('starten').onclick = () => befehl('starten').catch((e) => melden('hinweis', String(e)));
$('beenden').onclick = () => befehl('beenden').catch((e) => melden('hinweis', String(e)));

(async () => {
  const s = await befehl('einrichtung_stand');
  $('version').textContent = `Studio Kassenplatz ${s.version}`;
  if (s.hinweis) melden('hinweis', s.hinweis, 'warn');
  if (s.gekoppelt) fertigZeigen(s);
  else suchen();
})();
