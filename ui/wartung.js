// Wartungsmenü (Strg+Alt+S). Die PIN prüft der Studio-Server; danach ist das Menü zehn
// Minuten offen. Ohne gesetzte PIN gibt es kein Wartungsmenü – sie wird in der
// Kassenverwaltung unter „Kassenplätze“ festgelegt.

let pin = '';

function punkte() { $('punkte').textContent = '•'.repeat(pin.length); }

function pad() {
  const tasten = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '⌫', '0', 'OK'];
  for (const t of tasten) {
    const b = document.createElement('button');
    b.type = 'button';
    b.textContent = t;
    if (t === 'OK') b.className = 'blau';
    b.onclick = () => {
      if (t === '⌫') pin = pin.slice(0, -1);
      else if (t === 'OK') return pruefen();
      else if (pin.length < 8) pin += t;
      punkte();
    };
    $('pad').appendChild(b);
  }
}

async function pruefen() {
  melden('hinweis', '');
  try {
    if (await befehl('wartung_pin', { pin })) await menue();
    else { melden('hinweis', 'PIN falsch.'); pin = ''; punkte(); }
  } catch (e) { melden('hinweis', String(e)); pin = ''; punkte(); }
}

async function menue() {
  const s = await befehl('wartung_info');
  zeigen('sperre', false);
  zeigen('menue', true);
  $('i_edge').textContent = `${s.edge_name || ''} · ${s.edge || '–'}${s.offline ? ' (nicht erreichbar)' : ''}`;
  $('i_platz').textContent = `Kassenplatz ${s.platz_nr} · ${s.platz_name} (${ARTEN[s.art] || s.art})`;
  $('i_version').textContent = s.update ? `${s.version} · Update ${s.update} geladen` : s.version;
  $('update').textContent = s.update ? `Update ${s.update} jetzt installieren` : 'Nach Updates suchen';
  geraeteListe($('usb'), s.usb);
}

async function los(knopf, fn) {
  knopf.disabled = true;
  melden('hinweis', '');
  try { await fn(); } catch (e) { melden('hinweis', String(e)); } finally { knopf.disabled = false; }
}

$('abbrechen').onclick = () => befehl('zurueck');
$('zurueck').onclick = () => befehl('zurueck');
$('treiber').onclick = (e) => los(e.target, async () => { melden('hinweis', await befehl('geraete_einrichten'), 'gut'); await menue(); });
$('update').onclick = (e) => los(e.target, async () => {
  const v = await befehl('update_suchen');
  if (!v) { melden('hinweis', 'Die App ist aktuell.', 'gut'); return; }
  if (confirm(`Update ${v} jetzt installieren? Die App startet danach neu.`)) await befehl('update_installieren');
  else await menue();
});
$('edge_aendern').onclick = (e) => los(e.target, async () => {
  await befehl('edge_aendern', { edge: $('neue_edge').value });
  melden('hinweis', 'Adresse übernommen.', 'gut');
  await menue();
});
$('entkoppeln').onclick = (e) => los(e.target, async () => {
  if (confirm('Platz wirklich entkoppeln? Danach muss er neu gekoppelt werden.')) await befehl('entkoppeln');
});
$('beenden').onclick = (e) => los(e.target, () => befehl('beenden'));

(async () => {
  pad();
  const s = await befehl('wartung_info');
  $('platz').textContent = s.platz_name ? `Kassenplatz ${s.platz_nr} · ${s.platz_name}` : '';
  if (s.wartung_offen || !s.gekoppelt) { await menue(); return; }
  if (!s.wartungs_pin) $('pin_text').textContent = 'Für diesen Platz ist noch keine Wartungs-PIN festgelegt – in der Kassenverwaltung unter „Kassenplätze“ setzen.';
})();
