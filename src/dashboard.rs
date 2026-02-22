//! Embedded HTML dashboard for the Conduit Registry.

use axum::response::Html;

pub const DASHBOARD_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Conduit Registry</title>
<link rel="icon" href="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'><text y='.9em' font-size='90'>⚡</text></svg>">
<style>
  *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
  body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
         background: #0a0a0a; color: #e0e0e0; min-height: 100vh; }
  header { padding: 2rem 2rem 1rem; border-bottom: 1px solid #222; }
  header h1 { font-size: 1.4rem; font-weight: 600; color: #f7931a; letter-spacing: 0.03em; }
  header p { color: #888; font-size: 0.85rem; margin-top: 0.3rem; }
  .stats { display: flex; gap: 2rem; padding: 1.2rem 2rem; border-bottom: 1px solid #181818; }
  .stat { display: flex; flex-direction: column; }
  .stat-val { font-size: 1.6rem; font-weight: 700; color: #fff; }
  .stat-label { font-size: 0.75rem; color: #666; text-transform: uppercase; letter-spacing: 0.06em; }
  main { padding: 1.5rem 2rem; }
  .empty { color: #555; font-style: italic; padding: 3rem 0; text-align: center; }
  table { width: 100%; border-collapse: collapse; font-size: 0.85rem; }
  th { text-align: left; color: #666; font-weight: 500; padding: 0.6rem 0.8rem;
       border-bottom: 1px solid #222; text-transform: uppercase; font-size: 0.72rem;
       letter-spacing: 0.05em; }
  td { padding: 0.7rem 0.8rem; border-bottom: 1px solid #151515; vertical-align: top; }
  tr:hover td { background: #111; }
  .name { color: #fff; font-weight: 500; }
  .hash { font-family: 'SF Mono', 'Fira Code', monospace; font-size: 0.78rem; color: #666; }
  .price { color: #f7931a; font-weight: 600; white-space: nowrap; }
  .size { color: #aaa; }
  .chunks { color: #aaa; }
  .creator { font-family: 'SF Mono', 'Fira Code', monospace; font-size: 0.72rem; color: #555; }
  .seeders-badge { display: inline-block; padding: 0.15rem 0.5rem; border-radius: 9999px;
                   font-size: 0.72rem; font-weight: 600; }
  .seeders-0 { background: #1a1a1a; color: #555; }
  .seeders-n { background: #1a2f1a; color: #4ade80; }
  .merkle { font-family: 'SF Mono', 'Fira Code', monospace; font-size: 0.7rem; color: #444; }
  @media (max-width: 900px) {
    .hide-mobile { display: none; }
    .stats { flex-wrap: wrap; gap: 1rem; }
  }
</style>
</head>
<body>
<header>
  <h1>Conduit Registry</h1>
  <p>Content discovery index</p>
</header>
<div class="stats">
  <div class="stat"><span class="stat-val" id="listing-count">-</span><span class="stat-label">Listings</span></div>
  <div class="stat"><span class="stat-val" id="seeder-count">-</span><span class="stat-label">Seeder Announcements</span></div>
</div>
<main id="content"><p class="empty">Loading...</p></main>
<script>
async function load() {
  const [listRes, seederRes] = await Promise.all([
    fetch('/api/listings').then(r => r.json()),
    fetch('/api/seeders?all=1').then(r => r.json()).catch(() => ({items:[]}))
  ]);
  const listings = listRes.items || [];
  const seeders = seederRes.items || [];

  document.getElementById('listing-count').textContent = listings.length;
  document.getElementById('seeder-count').textContent = seeders.length;

  const main = document.getElementById('content');
  if (!listings.length) { main.innerHTML = '<p class="empty">No content registered yet.</p>'; return; }

  // Count seeders per encrypted_hash
  const seederMap = {};
  seeders.forEach(s => { seederMap[s.encrypted_hash] = (seederMap[s.encrypted_hash]||0) + 1; });

  function short(h) { return h ? h.slice(0,8) + '...' + h.slice(-6) : '-'; }
  function fmtSize(b) {
    if (b < 1024) return b + ' B';
    if (b < 1048576) return (b/1024).toFixed(1) + ' KB';
    return (b/1048576).toFixed(1) + ' MB';
  }

  let html = `<table>
    <tr>
      <th>Name</th>
      <th>Price</th>
      <th>Size</th>
      <th>Chunks</th>
      <th>Seeders</th>
      <th class="hide-mobile">Creator</th>
      <th class="hide-mobile">Content Hash</th>
    </tr>`;

  listings.forEach(l => {
    const sc = seederMap[l.encrypted_hash] || 0;
    const badge = sc > 0
      ? `<span class="seeders-badge seeders-n">${sc}</span>`
      : `<span class="seeders-badge seeders-0">0</span>`;
    html += `<tr>
      <td class="name">${l.file_name}</td>
      <td class="price">${l.price_sats} sats</td>
      <td class="size">${fmtSize(l.size_bytes)}</td>
      <td class="chunks">${l.chunk_count}</td>
      <td>${badge}</td>
      <td class="creator hide-mobile" title="${l.creator_pubkey}">${short(l.creator_pubkey)}</td>
      <td class="hash hide-mobile" title="${l.content_hash}">${short(l.content_hash)}</td>
    </tr>`;
  });
  html += '</table>';
  main.innerHTML = html;
}
load();
setInterval(load, 10000);
</script>
</body>
</html>"##;

pub async fn dashboard() -> Html<&'static str> {
    Html(DASHBOARD_HTML)
}
