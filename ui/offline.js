// Keine Verbindung: Der Agent sucht weiter. Sobald der Server antwortet, öffnet er die
// Oberfläche selbst neu. Diese Seite zeigt nur den Stand.
async function pruefen() {
  try {
    const s = await befehl('einrichtung_stand');
    $('detail').textContent = s.edge ? `${s.edge_name || 'Studio-Server'} · ${s.edge}` : '';
    if (s.gekoppelt && !s.offline) await befehl('starten');
  } catch (e) { /* weiter warten */ }
}
pruefen();
setInterval(pruefen, 10000);
