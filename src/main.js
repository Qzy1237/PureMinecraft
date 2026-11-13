// ============================================================================
//  PureMinecraft 前端
// ============================================================================

(() => {
  'use strict';
  const TAURI = window.__TAURI__ || null;
  const invoke = TAURI && TAURI.core ? TAURI.core.invoke : null;
  const tauriWin = (TAURI && TAURI.window) ? TAURI.window.getCurrentWindow() : null;
  const $ = id => document.getElementById(id);

  let localVersions = [];
  let selectedVersion = null;
  let remoteVersions = [];
  let pollTimer = null;
  let busy = false;

  // ---------------------------------------------------------------- 日志
  function log(msg, cls) {
    const el = document.createElement('div');
    if (cls) el.className = cls;
    const t = new Date().toLocaleTimeString('zh-CN', { hour12: false });
    el.textContent = `[${t}] ${msg}`;
    $('log').appendChild(el);
    $('log').scrollTop = $('log').scrollHeight;
  }

  // ---------------------------------------------------------------- 标签页
  document.querySelectorAll('.tab').forEach(t => {
    t.addEventListener('click', () => {
      document.querySelectorAll('.tab').forEach(x => x.classList.remove('on'));
      document.querySelectorAll('.page').forEach(x => x.classList.remove('on'));
      t.classList.add('on');
      $('page-' + t.dataset.tab).classList.add('on');
      if (t.dataset.tab === 'download') loadRemote();
      if (t.dataset.tab === 'versions') loadLocal();
      if (t.dataset.tab === 'mods') { if (!modsLoaded) doModSearch(); }
    });
  });

  // ---------------------------------------------------------------- 版本
  async function loadLocal() {
    if (!invoke) return;
    localVersions = await invoke('list_local');
    const box = $('localList');
    box.innerHTML = '';
    if (!localVersions.length) {
      box.innerHTML = '<div class="empty">还没有本地版本<br>去「资源下载」装一个</div>';
      return;
    }
    for (const v of localVersions) {
      const el = document.createElement('div');
      el.className = 'vitem' + (v === selectedVersion ? ' on' : '');
      const isFabric = v.toLowerCase().includes('fabric');
      el.innerHTML = `<span class="name">${v}</span>
        <span class="tag installed">已安装</span>
        ${isFabric ? '<span class="tag">Fabric</span>' : ''}`;
      el.addEventListener('click', () => {
        selectedVersion = v;
        localStorage.setItem('pcmc.version', v);
        renderCurrent();
        loadLocal();
        log('选择版本：' + v);
      });
      box.appendChild(el);
    }
    // 默认选中
    if (!selectedVersion && localVersions.length) {
      selectedVersion = localVersions[0];
      renderCurrent();
    }
  }

  function renderCurrent() {
    $('curVersion').textContent = selectedVersion || '未选择';
    $('curVersionSub').textContent = selectedVersion
      ? '点击下方按钮开始游戏'
      : '在「版本管理」里选一个';
  }

  async function loadRemote() {
    if (!invoke) return;
    if (remoteVersions.length) return renderRemote();
    $('remoteList').innerHTML = '<div class="empty">正在获取版本列表…</div>';
    try {
      const m = await invoke('get_manifest');
      remoteVersions = (m.versions || []).filter(v => v.type === 'release').slice(0, 120);
      log(`获取到 ${remoteVersions.length} 个正式版`);
    } catch (e) {
      log('获取版本列表失败：' + e, 'err');
      remoteVersions = [];
    }
    renderRemote();
  }

  function renderRemote() {
    const box = $('remoteList');
    box.innerHTML = '';
    if (!remoteVersions.length) {
      box.innerHTML = '<div class="empty">获取失败，检查网络后重试</div>';
      return;
    }
    for (const v of remoteVersions) {
      const el = document.createElement('div');
      el.className = 'vitem';
      const installed = localVersions.includes(v.id);
      el.innerHTML = `<span class="name">${v.id}</span>
        <span class="tag ${installed ? 'installed' : ''}">${installed ? '已安装' : v.releaseTime?.slice(0, 10) || ''}</span>`;
      if (!installed) {
        el.addEventListener('click', () => installVersion(v.id));
      }
      box.appendChild(el);
    }
  }

  async function installVersion(id) {
    if (busy || !invoke) return;
    busy = true;
    $('progress').hidden = false;
    log('开始安装 ' + id);
    const r = await invoke('install_version', { id });
    if (r !== 'started') {
      log('安装启动失败: ' + r, 'err');
      busy = false;
      return;
    }
    pollTimer = setInterval(poll, 600);
  }

  let lastProgMsg = '';
  async function poll() {
    let s;
    try { s = JSON.parse(await invoke('read_progress')); } catch (e) { return; }

    const pct = s.total ? Math.min(100, (100 * (s.done || 0)) / s.total) : (s.phase === 'done' ? 100 : 0);
    $('pFill').style.width = pct.toFixed(1) + '%';
    $('pCount').textContent = `${s.done || 0} / ${s.total || 0}`;
    $('pPhase').textContent = ({
      meta: '获取版本信息', client: '下载 client.jar', files: '下载依赖与资源',
      fabric: '安装 Fabric', done: '完成', error: '出错', launched: '已启动'
    })[s.phase] || s.phase;
    $('pFile').textContent = s.current || s.message || '';
    $('pFailed').textContent = s.failed ? `失败 ${s.failed}` : '';

    if (s.phase === 'done' || s.phase === 'error') {
      clearInterval(pollTimer);
      pollTimer = null;
      busy = false;
      const msg = s.message || (s.phase === 'done' ? '完成' : '失败');
      if (msg !== lastProgMsg) { lastProgMsg = msg; log(msg, s.phase === 'error' ? 'err' : 'ok'); }
      if (s.phase === 'done') {
        await loadLocal();
        remoteVersions = [];
        setTimeout(() => { $('progress').hidden = true; }, 3000);
      }
    }
  }

  // ---------------------------------------------------------------- 启动
  $('btnPlay').addEventListener('click', async () => {
    if (!invoke) return;
    if (!selectedVersion) { log('先选一个版本', 'err'); return; }
    const name = $('nick').value.trim() || 'Steve';
    const ram = parseInt($('ram').value, 10);
    const javaPath = $('javaPath').value.trim();
    localStorage.setItem('pcmc.nick', name);
    localStorage.setItem('pcmc.ram', String(ram));
    localStorage.setItem('pcmc.java', javaPath);
    log(`启动 ${selectedVersion} · ${name} · ${ram}MB`);
    const gameLang = localStorage.getItem('pcmc.gamelang') || 'zh_cn';
    const r = await invoke('launch', { id: selectedVersion, name, ramMb: ram, javaPath, lang: gameLang });
    log(r, r.startsWith('started') ? 'ok' : 'err');
    if (r === 'started') {
      const s = JSON.parse(await invoke('read_progress'));
      log(s.message || '', 'ok');
    } else {
      log(r, 'err');
    }
  });

  $('ram').addEventListener('input', e => {
    $('ramVal').textContent = (e.target.value / 1024).toFixed(1).replace('.0', '') + ' GB';
  });

  $('btnAutoJava').addEventListener('click', async () => {
    const t = await invoke('tool_status');
    $('javaPath').value = t.java || '';
    log('Java: ' + t.java);
  });

  $('btnOpenVersions').addEventListener('click', () => invoke('open_dir', { which: 'versions' }));

  $('btnCustomInstall').addEventListener('click', () => {
    const v = $('customVer').value.trim();
    if (!v) { log('先输入版本号', 'err'); return; }
    installVersion(v);
  });

  $('btnFabric').addEventListener('click', async () => {
    const mc = $('fabricMc').value.trim();
    const loader = $('fabricLoader').value.trim();
    if (!mc || !loader) { log('填 Minecraft 版本和 loader 版本', 'err'); return; }
    $('progress').hidden = false;
    log(`安装 Fabric ${loader} for ${mc}`);
    await invoke('install_fabric', { mc, loader });
    pollTimer = setInterval(poll, 600);
  });

  // ---------------------------------------------------------------- 模组详情
  let curDetail = null;

  const esc = s => String(s).replace(/[&<>"]/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}[c]));

  // 极简 markdown -> html（够用就行，先转义再替换）
  function mdToHtml(src) {
    if (!src) return '<p>（这个模组没有填写简介）</p>';
    let s = esc(src);
    s = s.replace(/```([\s\S]*?)```/g, (m, c) => `<pre><code>${c}</code></pre>`);
    s = s.replace(/!\[[^\]]*\]\(([^)]+)\)/g, '<img src="$1" />');
    s = s.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" target="_blank">$1</a>');
    s = s.replace(/^### (.*)$/gm, '<h3>$1</h3>');
    s = s.replace(/^## (.*)$/gm, '<h2>$1</h2>');
    s = s.replace(/^# (.*)$/gm, '<h1>$1</h1>');
    s = s.replace(/\*\*([^*]+)\*\*/g, '<b>$1</b>');
    s = s.replace(/`([^`]+)`/g, '<code>$1</code>');
    s = s.replace(/^&gt; (.*)$/gm, '<blockquote>$1</blockquote>');
    s = s.replace(/^[-*] (.*)$/gm, '<li>$1</li>');
    s = s.replace(/(?:<li>[\s\S]*?<\/li>\s*)+/g, m => '<ul>' + m.trim() + '</ul>');
    s = s.replace(/\n{2,}/g, '</p><p>');
    s = s.replace(/\n/g, '<br>');
    return '<p>' + s + '</p>';
  }

  function findVideos(body) {
    const out = [];
    if (!body) return out;
    const re = /(?:youtube\.com\/watch\?v=|youtu\.be\/|youtube\.com\/embed\/)([\w-]{11})/g;
    let m;
    while ((m = re.exec(body)) !== null) {
      if (!out.includes(m[1])) out.push(m[1]);
    }
    const bili = /bilibili\.com\/video\/(BV[\w]+)/g;
    while ((m = bili.exec(body)) !== null) {
      if (!out.includes(m[1])) out.push(m[1]);
    }
    return out;
  }

  async function openDetail(id) {
    if (!invoke) return;
    const d = $('detail');
    d.classList.add('open');
    d.setAttribute('aria-hidden', 'false');
    $('dTitle').textContent = '加载中…';
    $('dAuthor').textContent = '';
    $('dStats').textContent = '';
    $('dBody').innerHTML = '<p>加载中…</p>';
    $('dGallery').innerHTML = '';
    $('dVideo').innerHTML = '';
    $('dComments').innerHTML = '';
    switchDetailTab('desc');

    try {
      const raw = await invoke('mod_detail', { projectId: id });
      const p = JSON.parse(raw);
      if (p.error) { $('dBody').innerHTML = '<p>加载失败：' + esc(p.error) + '</p>'; return; }
      curDetail = p;
      $('dCrumb').textContent = p.title || '详情';
      $('dTitle').textContent = p.title || '';
      $('dAuthor').textContent = 'by ' + (p.team || '未知作者');
      $('dIcon').src = p.icon_url || '';
      $('dStats').innerHTML =
        `<span>下载 <b>${fmtNum(p.downloads || 0)}</b></span>` +
        `<span>关注 <b>${fmtNum(p.followers || 0)}</b></span>` +
        `<span>${(p.categories || []).join(' · ')}</span>` +
        `<span>${(p.loaders || []).join(' / ')}</span>`;

      // 简介
      $('dBody').innerHTML = mdToHtml(p.body || '');

      // 图集
      const g = (p.gallery || []).filter(x => x && x.url);
      $('dGallery').innerHTML = g.length
        ? g.map(x => `<img src="${esc(x.url)}" title="${esc(x.title || '')}" />`).join('')
        : '<div class="notice">作者没有上传图片</div>';

      // 视频
      const vids = findVideos(p.body);
      if (vids.length) {
        $('dVideo').innerHTML = vids.map(v => {
          if (v.length === 11) {
            return `<iframe src="https://www.youtube.com/embed/${v}" allowfullscreen></iframe>`;
          }
          return `<iframe src="https://player.bilibili.com/player.html?bvid=${v}&high_quality=1" allowfullscreen></iframe>`;
        }).join('');
      } else {
        const q = encodeURIComponent((p.title || '') + ' Minecraft 模组');
        $('dVideo').innerHTML =
          '<div class="notice">这个模组的简介里没有内嵌宣传视频。<br>' +
          '（Modrinth 没有独立的视频字段，只能从简介正文里识别 YouTube / Bilibili 链接）</div>' +
          `<div class="video-links">` +
          `<button class="mini" data-url="https://search.bilibili.com/all?keyword=${q}">在 B 站搜索</button>` +
          `<button class="mini" data-url="https://www.youtube.com/results?search_query=${q}">在 YouTube 搜索</button>` +
          `</div>`;
        document.querySelectorAll('#dVideo [data-url]').forEach(b =>
          b.addEventListener('click', () => invoke('open_url', { url: b.dataset.url })));
      }

      // 评论
      $('dComments').innerHTML =
        '<div class="notice">Modrinth 没有公开的评论接口（只有关注数和下载量），所以这里无法展示真实评论。<br><br>' +
        `当前关注数：<b>${fmtNum(p.followers || 0)}</b>　下载数：<b>${fmtNum(p.downloads || 0)}</b><br><br>` +
        (p.issues_url ? `<a href="${esc(p.issues_url)}" target="_blank">去 GitHub Issues 反馈问题</a><br>` : '') +
        (p.source_url ? `<a href="${esc(p.source_url)}" target="_blank">查看源码</a><br>` : '') +
        `<a href="https://modrinth.com/mod/${esc(p.slug || '')}" target="_blank">打开 Modrinth 项目页</a>` +
        '</div>';

      $('dInstall').onclick = async () => {
        const mc = (selectedVersion || '').split('-').filter(x => /^\d+\.\d+/.test(x)).pop() || '1.20.1';
        const loader = $('modLoader').value;
        $('dInstall').disabled = true;
        $('dInstall').textContent = '安装中…';
        log(`安装模组 ${p.title}（${loader} / MC ${mc}）`);
        try {
          const r = await invoke('install_mod', { projectId: id, mc, loader });
          log(r, r.startsWith('已安装') || r.startsWith('已存在') ? 'ok' : 'err');
          $('dInstall').textContent = r.startsWith('已安装') || r.startsWith('已存在') ? '已安装' : '重试';
        } catch (e) {
          log('安装失败：' + e, 'err');
          $('dInstall').textContent = '重试';
        }
        $('dInstall').disabled = false;
      };
    } catch (e) {
      $('dBody').innerHTML = '<p>加载失败：' + esc(e) + '</p>';
    }
  }

  function switchDetailTab(name) {
    document.querySelectorAll('.dtab').forEach(b =>
      b.classList.toggle('on', b.dataset.tab === name));
    document.querySelectorAll('.dpage').forEach(p =>
      p.classList.toggle('on', p.id === 'dp-' + name));
  }

  document.querySelectorAll('.dtab').forEach(b =>
    b.addEventListener('click', () => switchDetailTab(b.dataset.tab)));

  $('detailBack').addEventListener('click', () => {
    $('detail').classList.remove('open');
    $('detail').setAttribute('aria-hidden', 'true');
  });

  // ---------------------------------------------------------------- 模组库
  let modsLoaded = false;
  let modOffset = 0;
  let modTotal = 0;
  let modBusy = false;

  const fmtNum = n => {
    if (n >= 1e9) return (n / 1e9).toFixed(1) + 'B';
    if (n >= 1e6) return (n / 1e6).toFixed(1) + 'M';
    if (n >= 1e3) return (n / 1e3).toFixed(1) + 'K';
    return String(n);
  };

  function renderMods(hits, append) {
    const box = $('modList');
    if (!append) box.innerHTML = '';
    if (!hits.length && !append) {
      box.innerHTML = '<div class="empty">没有结果</div>';
      return;
    }
    for (const m of hits) {
      const el = document.createElement('div');
      el.className = 'mod-item';
      const icon = m.icon_url || 'data:image/svg+xml;utf8,<svg xmlns="http://www.w3.org/2000/svg"/>';
      el.innerHTML = `
        <img class="mod-icon" src="${icon}" loading="lazy" />
        <div class="mod-body">
          <div class="mod-title">${m.title}<span class="author">@${(m.author || '').replace(/</g,'')}</span></div>
          <div class="mod-desc">${(m.description || '').replace(/</g,'')}</div>
          <div class="mod-meta">
            <span>下载 <b>${fmtNum(m.downloads || 0)}</b></span>
            <span>评分 <b>${fmtNum(m.follows || 0)}</b></span>
            <span>${(m.categories || []).slice(0, 3).join(' · ')}</span>
          </div>
        </div>
        <div class="mod-side">
          <button class="mod-dl">安装</button>
        </div>`;
      const btn = el.querySelector('.mod-dl');
      btn.addEventListener('click', (ev) => { ev.stopPropagation(); installMod(m.project_id, m.title, btn); });
      // 点卡片进入二级详情页
      el.addEventListener('click', () => openDetail(m.project_id));
      el.style.cursor = 'pointer';
      box.appendChild(el);
    }
  }

  async function doModSearch(append) {
    if (!invoke || modBusy) return;
    modBusy = true;
    const q = $('modQuery').value.trim();
    const loader = $('modLoader').value;
    const sort = $('modSort').value;
    if (!append) { modOffset = 0; $('modList').innerHTML = '<div class="empty">搜索中…</div>'; }
    try {
      const raw = await invoke('search_mods', { query: q, loader, index: modOffset, sort });
      const data = JSON.parse(raw);
      modTotal = data.total_hits || 0;
      modOffset += (data.hits || []).length;
      $('modCount').textContent = `Modrinth · 共 ${fmtNum(modTotal)} 个`;
      renderMods(data.hits || [], append);
      modsLoaded = true;
      if (append) log(`加载更多，已显示 ${modOffset}/${modTotal}`);
      else log(`搜索「${q || 'mod'}」(${loader})：${modTotal} 个结果`);
    } catch (e) {
      log('模组搜索失败：' + e, 'err');
    }
    modBusy = false;
  }

  async function installMod(projectId, title, btn) {
    const mc = (selectedVersion || '').split('-')[0] || '1.20.1';
    const loader = $('modLoader').value;
    btn.disabled = true;
    btn.textContent = '…';
    log(`安装模组 ${title}（${loader} / MC ${mc}）`);
    try {
      const r = await invoke('install_mod', { projectId, mc, loader });
      log(r, r.startsWith('已安装') || r.startsWith('已存在') ? 'ok' : 'err');
      btn.textContent = r.startsWith('已安装') || r.startsWith('已存在') ? '已装' : '重试';
    } catch (e) {
      log('安装失败：' + e, 'err');
      btn.textContent = '重试';
    }
    btn.disabled = false;
  }

  $('btnModSearch').addEventListener('click', () => doModSearch(false));
  $('modQuery').addEventListener('keydown', e => { if (e.key === 'Enter') doModSearch(false); });
  $('modSort').addEventListener('change', () => doModSearch(false));
  $('modLoader').addEventListener('change', () => doModSearch(false));
  $('modList').addEventListener('scroll', () => {
    const b = $('modList');
    if (b.scrollTop + b.clientHeight > b.scrollHeight - 80 && modOffset < modTotal && !modBusy) {
      doModSearch(true);
    }
  });

  // ---------------------------------------------------------------- 设置
  function applyTheme(theme) {
    document.body.classList.toggle('light', theme === 'light');
    document.querySelectorAll('#themeSeg button').forEach(b =>
      b.classList.toggle('active', b.dataset.theme === theme));
    localStorage.setItem('pcmc.theme', theme);
  }
  document.querySelectorAll('#themeSeg button').forEach(b => {
    b.addEventListener('click', () => applyTheme(b.dataset.theme));
  });

  let lang = localStorage.getItem('pcmc.lang') || 'zh-CN';
  document.querySelectorAll('#langSeg button').forEach(b => {
    b.addEventListener('click', () => {
      lang = b.dataset.lang;
      localStorage.setItem('pcmc.lang', lang);
      document.querySelectorAll('#langSeg button').forEach(x =>
        x.classList.toggle('active', x.dataset.lang === lang));
      log('语言切换：' + lang);
    });
  });

  // 窗口置顶
  async function applyTop(on) {
    $('swTop').setAttribute('aria-checked', on ? 'true' : 'false');
    localStorage.setItem('pcmc.ontop', on ? '1' : '0');
    if (invoke) {
      try { await invoke('set_always_on_top', { on }); } catch (e) {}
    }
  }
  $('swTop').addEventListener('click', () => {
    const cur = $('swTop').getAttribute('aria-checked') === 'true';
    applyTop(!cur);
    log('窗口置顶：' + (!cur ? '开' : '关'));
  });

  // 游戏内语言
  let gameLang = localStorage.getItem('pcmc.gamelang') || 'zh_cn';
  function renderGameLang() {
    document.querySelectorAll('#gameLangSeg button').forEach(b =>
      b.classList.toggle('active', b.dataset.lang === gameLang));
  }
  document.querySelectorAll('#gameLangSeg button').forEach(b => {
    b.addEventListener('click', () => {
      gameLang = b.dataset.lang;
      localStorage.setItem('pcmc.gamelang', gameLang);
      renderGameLang();
      log('游戏内语言：' + (gameLang === 'zh_cn' ? '简体中文' : 'English'));
    });
  });

  $('javaCancel').addEventListener('click', () => {
    $('javaModal').classList.remove('open');
    log('已取消，需要手动指定 Java 路径');
  });
  $('javaDownload').addEventListener('click', async () => {
    $('javaModal').classList.remove('open');
    $('progress').hidden = false;
    log('开始下载 Java 运行环境（约 178 MB）…');
    $('pPhase').textContent = '下载 Java';
    if (pollTimer) clearInterval(pollTimer);
    pollTimer = setInterval(poll, 800);
    const path = await invoke('download_java');
    clearInterval(pollTimer);
    pollTimer = null;
    if (path) {
      $('javaPath').value = path;
      $('javaState').textContent = '已就绪';
      $('javaState').style.color = '';
      log('Java 已下载：' + path, 'ok');
    } else {
      log('Java 下载失败，请手动指定', 'err');
    }
    setTimeout(() => { $('progress').hidden = true; }, 3000);
  });

  $('btnClearLog').addEventListener('click', () => { $('log').innerHTML = ''; });

  // ---------------------------------------------------------------- 崩溃分析
  let lastCrash = null;

  const CRASH_FRIENDLY = {
    out_of_memory: '内存不足，建议在启动器设置中调大内存分配。',
    java_too_old: 'Java 版本过低，请更换高版本 Java。',
    missing_class: '缺少前置模组或版本不兼容，请检查。',
    mod_conflict: '模组之间版本冲突或缺少依赖，请检查 mods 文件夹。',
    graphics: '显卡驱动过旧或不被支持，请更新显卡驱动。',
    file_lock: '游戏文件被占用，请关闭另一个 Minecraft 后重试。'
  };

  async function showCrashAnalysis(exitCode) {
    if (!invoke) return;
    $('crashModal').classList.add('open');
    $('crashModal').setAttribute('aria-hidden', 'false');
    $('crashTitle').textContent = `游戏异常退出（Exit Code ${exitCode}）`;
    $('crashHint').className = 'crash-hint';
    $('crashHint').textContent = '正在读取崩溃报告…';
    $('crashFile').textContent = '';
    $('crashAiBox').hidden = true;
    $('crashAi').hidden = true;
    $('crashAiResult').hidden = true;
    $('crashAiResult').textContent = '';

    let r;
    try {
      r = await invoke('analyze_crash');
    } catch (e) {
      $('crashHint').textContent = '读取崩溃报告失败：' + e;
      return;
    }
    lastCrash = r;

    if (!r.found) {
      $('crashHint').className = 'crash-hint miss';
      $('crashHint').textContent =
        '没有找到崩溃报告文件。\n' +
        '可能游戏还没到写日志的阶段就退出了，建议直接看 logs 目录。';
    } else if (r.matched && r.matched.length) {
      const first = r.matched[0];
      $('crashHint').className = 'crash-hint hit';
      $('crashHint').textContent = CRASH_FRIENDLY[first.id] || first.advice || first.title;
      if (r.matched.length > 1) {
        $('crashHint').textContent += '\n\n同时匹配到：' + r.matched.slice(1).map(m => m.title).join('、');
      }
    } else {
      $('crashHint').className = 'crash-hint miss';
      $('crashHint').textContent =
        '本地规则库没有识别出明确原因。\n可以点下面的「使用 AI 深度分析」，或者打开日志目录自己看看。';
    }

    if (r.found) {
      $('crashFile').textContent = `日志：${r.file}（${r.line_count} 行）`;
    }

    // 只有「有 Key」且「本地没命中」才显示 AI 按钮
    try {
      const cfg = await invoke('ai_config_get');
      if (cfg.has_key && r.found && !(r.matched && r.matched.length)) {
        $('crashAiBox').hidden = false;
        $('crashAi').hidden = false;
      }
    } catch (_) { /* 没配置就什么都不做 */ }
  }

  $('crashClose').addEventListener('click', () => {
    $('crashModal').classList.remove('open');
    $('crashModal').setAttribute('aria-hidden', 'true');
  });

  $('crashOpenLog').addEventListener('click', async () => {
    if (invoke) await invoke('open_crash_dir');
  });

  $('crashAi').addEventListener('click', async () => {
    if (!invoke || !lastCrash) return;
    const cfg = await invoke('ai_config_get');
    if (!cfg.has_key) { log('还没配置 API Key', 'err'); return; }

    $('crashAi').disabled = true;
    $('crashAi').textContent = '分析中…';
    $('crashAiResult').hidden = false;
    $('crashAiResult').textContent = '正在请求 AI，请稍候（不会阻塞界面）…';
    log('调用 AI 分析崩溃日志（仅发送最后 50 行）');

    try {
      const text = await invoke('ai_analyze', {
        apiKey: cfg.key,
        apiUrl: cfg.url,
        logTail: lastCrash.tail || '',
        model: cfg.model || 'deepseek-chat'
      });
      $('crashAiResult').textContent = text;
      log('AI 分析完成', 'ok');
    } catch (e) {
      $('crashAiResult').textContent = '分析失败：' + e;
      log('AI 分析失败：' + e, 'err');
    }
    $('crashAi').disabled = false;
    $('crashAi').textContent = '使用 AI 深度分析';
  });

  // ---------------------------------------------------------------- AI 配置
  async function loadAiConfig() {
    if (!invoke) return;
    try {
      const c = await invoke('ai_config_get');
      $('aiKey').value = c.key || '';
      $('aiUrl').value = c.url || 'https://api.deepseek.com/chat/completions';
      $('aiModel').value = c.model || 'deepseek-chat';
      $('aiState').textContent = c.has_key ? '已配置（本地规则未命中时可调用）' : '未配置（纯本地模式）';
      $('aiState').style.color = c.has_key ? '' : '';
    } catch (e) { /* 忽略 */ }
  }

  $('aiSave').addEventListener('click', async () => {
    if (!invoke) return;
    const key = $('aiKey').value.trim();
    const url = $('aiUrl').value.trim() || 'https://api.deepseek.com/chat/completions';
    const model = $('aiModel').value.trim() || 'deepseek-chat';
    const ok = await invoke('ai_config_set', { key, url, model });
    if (ok) {
      $('aiState').textContent = key ? '已配置（本地规则未命中时可调用）' : '未配置（纯本地模式）';
      log(key ? 'AI 配置已保存' : '已清空 API Key，回到纯本地模式', 'ok');
    } else {
      log('保存失败', 'err');
    }
  });
  // ---------------------------------------------------------------- 卸载
  const fmtBytes = n => n >= 1073741824 ? (n / 1073741824).toFixed(2) + ' GB'
                       : n >= 1048576 ? (n / 1048576).toFixed(1) + ' MB'
                       : (n / 1024).toFixed(0) + ' KB';

  $('btnUninstall').addEventListener('click', async () => {
    $('uninstModal').classList.add('open');
    $('uninstModal').setAttribute('aria-hidden', 'false');
    $('savesSummary').textContent = '正在统计存档…';
    document.querySelector('input[name=uninst][value=keep]').checked = true;
    if (!invoke) return;
    try {
      const s = await invoke('saves_info');
      if (s.exists) {
        $('savesSummary').innerHTML =
          `存档目录：<b>${s.path}</b><br>` +
          `共 <b>${s.count}</b> 个世界，占用 <b>${fmtBytes(s.size)}</b>` +
          (s.count ? `<br>${s.worlds.slice(0, 5).join('、')}${s.count > 5 ? ' …' : ''}` : '');
      } else {
        $('savesSummary').innerHTML = '当前还没有存档（.minecraft\\saves 不存在）';
      }
    } catch (e) {
      $('savesSummary').textContent = '统计存档失败：' + e;
    }
  });

  $('uninstCancel').addEventListener('click', () => {
    $('uninstModal').classList.remove('open');
    $('uninstModal').setAttribute('aria-hidden', 'true');
  });

  $('uninstOk').addEventListener('click', async () => {
    const purge = document.querySelector('input[name=uninst]:checked').value === 'purge';
    $('uninstModal').classList.remove('open');
    log(purge ? '卸载：同时删除存档' : '卸载：保留存档', 'ok');
    if (invoke) await invoke('uninstall_app', { purge });
    else if (tauriWin) tauriWin.close();
  });

  $('btn-close').addEventListener('click', () => tauriWin ? tauriWin.close() : window.close());
  $('btn-min').addEventListener('click', () => tauriWin && tauriWin.minimize());

  // ---------------------------------------------------------------- 启动
  async function init() {
    applyTheme(localStorage.getItem('pcmc.theme') || 'dark');
    $('nick').value = localStorage.getItem('pcmc.nick') || 'Steve';
    $('javaPath').value = localStorage.getItem('pcmc.java') || '';
    const ram = parseInt(localStorage.getItem('pcmc.ram') || '4096', 10);
    $('ram').value = ram;
    $('ramVal').textContent = (ram / 1024).toFixed(1).replace('.0', '') + ' GB';
    document.querySelectorAll('#langSeg button').forEach(x =>
      x.classList.toggle('active', x.dataset.lang === lang));

    renderGameLang();
    const onTop = localStorage.getItem('pcmc.ontop') === '1';
    $('swTop').setAttribute('aria-checked', onTop ? 'true' : 'false');
    if (!invoke) { log('Tauri 未就绪', 'err'); return; }
    if (onTop) { try { await invoke('set_always_on_top', { on: true }); } catch (e) {} }
    try {
      const t = await invoke('tool_status');
      $('gameDirState').textContent = t.game_dir;
      $('javaState').textContent = t.java_exists ? '已就绪' : '未找到';
      $('javaState').style.color = t.java_exists ? '' : '#FF9AB0';
      log('启动器目录：' + t.root);
      log('游戏目录：' + t.game_dir);
      if (t.java_exists && t.java) {
        // 自动填入 Java 路径。本地目录里探测到的优先，
        // 只有本地没探测到、且用户之前手动填过时才沿用旧值。
        const saved = $('javaPath').value.trim();
        if (t.java_from_local || !saved) {
          $('javaPath').value = t.java;
        } else if (!saved.includes(t.java)) {
          $('javaPath').value = t.java;   // 旧值指向别的机器，直接替换
        }
        log('Java：' + t.java + '（已就绪）', 'ok');
      } else {
        $('javaPath').value = '';
        log('Java：未找到（本地 runtime 和系统 PATH 都没有）', 'err');
        $('javaModal').classList.add('open');
      }
    } catch (e) {}
    await loadLocal();
    await loadAiConfig();
    selectedVersion = localStorage.getItem('pcmc.version') || selectedVersion;
    renderCurrent();
    log('PureMinecraft 就绪');

    // 监听游戏退出：非 0 才弹崩溃分析
    if (tauriWin && tauriWin.listen) {
      await tauriWin.listen('game-exit', (ev) => {
        const code = ev.payload;
        log('游戏已退出，Exit Code ' + code, code === 0 ? 'ok' : 'err');
        if (code !== 0) showCrashAnalysis(code);
      });
    }
  }
  init();
})();
