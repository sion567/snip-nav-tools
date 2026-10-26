// SnipNav 前端 API 层
// - Tauri 环境：window.__TAURI__.core.invoke 直调 Rust 命令
// - 浏览器环境（开发调试）：自动切换内置 Mock，UI 可独立打开调试
(function () {
  const tauriCore = (window.__TAURI__ && window.__TAURI__.core) || null;

  async function invoke(cmd, args) {
    if (tauriCore) return tauriCore.invoke(cmd, args || {});
    return mockInvoke(cmd, args || {});
  }

  // ================= 浏览器 Mock（仅 UI 开发用，数据与 Rust 种子同源风格） =================

  const TODAY = '2026-10-25';

  const M = {
    nextId: 100,
    snippets: [
      { id: 1, tool: 'docker', command: 'logs', project: 'shop-admin', intent: 'docker logs 按时间+级别过滤', cmd: 'docker logs -f mycontainer --since 2026-10-25T00:00:00 | grep ERROR', variantOf: null, source: 'self', verified: true, usedCount: 12, lastUsed: TODAY, created: TODAY, notes: '示例记录：可随时删除', tags: ['docker', 'filter', 'log'] },
      { id: 2, tool: 'docker', command: 'logs', project: 'shop-admin', intent: 'docker logs 只看末尾 100 行', cmd: 'docker logs --tail 100 -t mycontainer', variantOf: null, source: 'self', verified: true, usedCount: 5, lastUsed: '2026-10-16', created: '2026-10-10', notes: '示例记录：可随时删除', tags: ['docker', 'log'] },
      { id: 3, tool: 'gradle', command: 'bootRun', project: 'tools-help', intent: 'gradle bootRun 指定端口启动', cmd: './gradlew bootRun --args="--server.port=8081"', variantOf: null, source: 'self', verified: true, usedCount: 8, lastUsed: '2026-10-22', created: '2026-09-30', notes: '示例记录：可随时删除', tags: ['gradle', 'network', 'run'] },
      { id: 4, tool: 'git', command: 'log', project: 'shop-admin', intent: 'git log 找包含关键字的提交', cmd: 'git log --oneline --grep="登录"', variantOf: null, source: 'self', verified: true, usedCount: 6, lastUsed: '2026-10-13', created: '2026-09-28', notes: '示例记录：可随时删除', tags: ['git', 'filter'] },
      { id: 5, tool: 'kubectl', command: 'logs', project: 'tools-help', intent: 'kubectl 看 pod 日志尾部', cmd: 'kubectl logs -f my-pod -n prod --tail=100', variantOf: null, source: 'self', verified: true, usedCount: 3, lastUsed: '2026-10-20', created: '2026-10-08', notes: '示例记录：可随时删除', tags: ['k8s', 'log'] },
      { id: 6, tool: 'netstat', command: null, project: 'tools-help', intent: '查端口占用（Windows）', cmd: 'netstat -ano | findstr :8080', variantOf: null, source: 'self', verified: true, usedCount: 9, lastUsed: '2026-10-24', created: '2026-09-25', notes: '示例记录：可随时删除', tags: ['network', 'windows'] },
      { id: 11, tool: 'grep', command: null, project: null, intent: '在文件中查找包含关键字的行', cmd: 'grep "ERROR" app.log', variantOf: null, source: 'tldr', verified: false, usedCount: 0, lastUsed: null, created: TODAY, notes: '在文件内容中查找匹配的文本行，支持正则表达式。', tags: ['filter', 'linux', 'log'] },
      { id: 12, tool: 'grep', command: null, project: null, intent: '忽略大小写并显示行号', cmd: 'grep -in "error" app.log', variantOf: null, source: 'tldr', verified: false, usedCount: 0, lastUsed: null, created: TODAY, notes: '在文件内容中查找匹配的文本行，支持正则表达式。', tags: ['filter', 'linux'] },
      { id: 13, tool: 'grep', command: null, project: null, intent: '显示匹配行的上下文各 3 行', cmd: 'grep -C 3 "ERROR" app.log', variantOf: null, source: 'tldr', verified: false, usedCount: 0, lastUsed: null, created: TODAY, notes: '在文件内容中查找匹配的文本行，支持正则表达式。', tags: ['filter', 'linux'] },
      { id: 21, tool: 'curl', command: null, project: null, intent: '发 POST 请求（JSON body）', cmd: 'curl -X POST -H "Content-Type: application/json" -d \'{"name":"test"}\' https://example.com/api', variantOf: null, source: 'tldr', verified: false, usedCount: 0, lastUsed: null, created: TODAY, notes: '向服务器发送 HTTP 请求，常用于调试接口。', tags: ['network'] },
      { id: 31, tool: 'docker', command: 'logs', project: null, intent: '实时跟随最新输出', cmd: 'docker logs -f --tail 100 {{容器}}', variantOf: null, source: 'tldr', verified: false, usedCount: 0, lastUsed: null, created: TODAY, notes: '查看容器的标准输出日志。', tags: ['docker', 'log'] },
      { id: 41, tool: 'tar', command: null, project: null, intent: 'tar：归档工具（手册页）', cmd: 'tar', variantOf: null, source: 'doc', verified: false, usedCount: 0, lastUsed: null, created: TODAY, notes: '手册页：点「📖 全文」查看整页', tags: ['build', 'linux'] },
    ],
    paramTree: [
      { id: 1, tool: 'docker', command: 'logs', category: 'time', flag: '--since <时间点>', desc: '从指定时间开始', example: '--since 2026-10-25T00:00:00' },
      { id: 2, tool: 'docker', command: 'logs', category: 'time', flag: '--since 1h', desc: '最近 1 小时', example: '--since 1h' },
      { id: 3, tool: 'docker', command: 'logs', category: 'time', flag: '--until <时间点>', desc: '到指定时间结束', example: '--until 2026-10-25T12:00:00' },
      { id: 4, tool: 'docker', command: 'logs', category: 'output', flag: '--tail <N>', desc: '只看最后 N 行', example: '--tail 100' },
      { id: 5, tool: 'docker', command: 'logs', category: 'output', flag: '-t, --timestamps', desc: '显示时间戳', example: '-t' },
      { id: 6, tool: 'docker', command: 'logs', category: 'pipe', flag: '| grep "ERROR"', desc: '只留错误日志', example: '| grep "ERROR"' },
      { id: 7, tool: 'docker', command: 'logs', category: 'pipe', flag: "| awk '{print $1}'", desc: '只取第一列（时间）', example: "| awk '{print $1}'" },
      { id: 8, tool: 'docker', command: 'logs', category: 'pipe', flag: "| jq '.level'", desc: 'JSON 日志过滤', example: "| jq '.level'" },
      { id: 11, tool: 'grep', command: '', category: 'filter', flag: '-i', desc: '忽略大小写', example: '-i' },
      { id: 12, tool: 'grep', command: '', category: 'filter', flag: '-n', desc: '显示行号', example: '-n' },
      { id: 13, tool: 'grep', command: '', category: 'filter', flag: '-v', desc: '反选（不含关键字）', example: '-v' },
      { id: 14, tool: 'grep', command: '', category: 'output', flag: '-C <N>', desc: '显示上下文 N 行', example: '-C 5' },
    ],
    patterns: [
      { id: 1, baseCmd: 'docker logs', suffix: '| grep "ERROR"', usedCount: 12 },
      { id: 2, baseCmd: 'docker logs', suffix: '| grep -C 5 "ERROR"', usedCount: 5 },
      { id: 3, baseCmd: 'kubectl logs', suffix: '--tail=100 -f', usedCount: 3 },
    ],
  };

  M.history = [
    { ts: '2026-10-26T10:12:01', cmd: 'docker logs --tail 100 -t mycontainer' },
    { ts: '2026-10-26T10:08:44', cmd: 'ffmpeg -i in.mp4 -vn -acodec copy out.m4a' },
    { ts: '2026-10-26T09:57:30', cmd: 'netstat -ano | findstr :8080' },
    { ts: '2026-10-26T09:51:12', cmd: 'git log --oneline --grep=登录' },
    { ts: '2026-10-26T09:40:03', cmd: 'tar -xzvf bundle.tar.gz -C /tmp/out' },
  ];
  M.settings = { llm_enabled: '0', llm_base_url: '', llm_api_key: '', llm_model: '' };
  M.autostart = false;

  const CATEGORY_LABELS = { time: '时间过滤', output: '输出控制', filter: '内容过滤', pipe: '管道过滤', run: '运行控制', other: '其他' };
  const CATEGORY_ORDER = ['time', 'output', 'filter', 'pipe', 'run'];

  function days(d) { return Math.floor((Date.parse(TODAY) - Date.parse(d)) / 86400000); }

  function cloneSn(s) { return JSON.parse(JSON.stringify(s)); }

  function autoTags(cmd, intent) {
    const rules = {
      docker: ['docker '], k8s: ['kubectl'], gradle: ['gradle', 'gradlew'], git: ['git '],
      windows: ['netstat', 'findstr', 'taskkill', 'netsh', 'powershell'],
      linux: ['grep', 'awk', 'sed'], log: ['log', 'logs'], filter: ['grep', 'findstr', '|'],
      run: ['run', 'bootrun'], network: ['port', 'netstat', 'curl'],
    };
    const hay = (cmd + ' ' + intent).toLowerCase();
    return Object.keys(rules).filter((t) => rules[t].some((k) => hay.includes(k)));
  }

  function deriveToolCommand(cmd) {
    const toks = (cmd.split('|')[0] || '').trim().split(/\s+/).filter(Boolean);
    if (!toks.length) return [null, null];
    const tool = taks(toks[0]);
    const command = toks.length > 1 && !/^[-/\\]/.test(toks[1]) && !/\.(sh|py|js)$/.test(toks[1]) ? toks[1] : null;
    return [tool, command];
  }
  function taks(t) { return (t === './gradlew' || t === 'gradlew') ? 'gradle' : t; }

  function mockScore(sn, kws, project) {
    let s = 0;
    if (sn.source === 'self') s += 50;
    if (project && sn.project && sn.project.toLowerCase() === project.toLowerCase()) s += 30;
    s += sn.usedCount * 2;
    if (sn.lastUsed && days(sn.lastUsed) >= 0 && days(sn.lastUsed) <= 7) s += 20;
    for (const kw of kws) {
      if ((sn.tags || []).includes(kw)) s += 10;
      if ((sn.intent || '').toLowerCase().includes(kw)) s += 5;
      if ((sn.cmd || '').toLowerCase().includes(kw) || ((sn.tool || '') + ' ' + (sn.command || '')).toLowerCase().includes(kw)) s += 2;
    }
    return s;
  }

  async function mockInvoke(cmd, args) {
    await new Promise((r) => setTimeout(r, 60)); // 模拟 IPC 延迟
    switch (cmd) {
      case 'search': {
        const kws = (args.q || '').split(/\s+/).map((w) => w.trim().toLowerCase()).filter(Boolean);
        let list = M.snippets.map((sn) => ({ sn, score: mockScore(sn, kws, args.project || null) }));
        if (kws.length) {
          list = list.filter(({ sn }) => kws.some((kw) =>
            (sn.intent || '').toLowerCase().includes(kw) || (sn.cmd || '').toLowerCase().includes(kw) ||
            (sn.notes || '').toLowerCase().includes(kw) || ((sn.tool || '') + ' ' + (sn.command || '')).toLowerCase().includes(kw) ||
            (sn.project || '').toLowerCase().includes(kw) || (sn.tags || []).some((t) => t.includes(kw))));
        } else {
          list.sort((a, b) => b.sn.usedCount - a.sn.usedCount);
        }
        if (kws.length) list.sort((a, b) => b.score - a.score || b.sn.usedCount - a.sn.usedCount);
        return list.slice(0, 50).map(({ sn, score }) => Object.assign(cloneSn(sn), { score }));
      }
      case 'get_snippet':
        return cloneSn(M.snippets.find((s) => s.id === args.id) || null);
      case 'create_snippet': {
        const p = args.payload;
        const [tool, command] = deriveToolCommand(p.cmd);
        const sn = {
          id: M.nextId++, tool, command, project: p.project || null,
          intent: p.intent, cmd: p.cmd, variantOf: p.variantOf || null,
          source: 'self', verified: true, usedCount: 0, lastUsed: null, created: TODAY,
          notes: p.notes || null,
          tags: Array.from(new Set([...(p.tags || []).map((t) => t.toLowerCase()), ...autoTags(p.cmd, p.intent)])).sort(),
        };
        M.snippets.unshift(sn);
        return cloneSn(sn);
      }
      case 'update_snippet': {
        const sn = M.snippets.find((s) => s.id === args.id);
        if (!sn) throw new Error('记录不存在');
        if (args.payload.intent) sn.intent = args.payload.intent;
        if (args.payload.cmd) {
          sn.cmd = args.payload.cmd;
          const [tool, command] = deriveToolCommand(sn.cmd);
          sn.tool = tool; sn.command = command;
        }
        sn.notes = args.payload.notes !== undefined ? (args.payload.notes || null) : sn.notes;
        sn.project = args.payload.project !== undefined ? (args.payload.project || null) : sn.project;
        if (args.payload.tags) sn.tags = args.payload.tags.map((t) => t.toLowerCase());
        return cloneSn(sn);
      }
      case 'delete_snippet':
        M.snippets = M.snippets.filter((s) => s.id !== args.id);
        return null;
      case 'copy_snippet': {
        const sn = M.snippets.find((s) => s.id === args.id);
        if (!sn) throw new Error('记录不存在');
        sn.usedCount++; sn.lastUsed = TODAY;
        return cloneSn(sn);
      }
      case 'copy_variant': {
        const sn = M.snippets.find((s) => s.id === args.id);
        if (!sn) throw new Error('记录不存在');
        const baseName = sn.tool ? (sn.command ? sn.tool + ' ' + sn.command : sn.tool) : sn.cmd.split(' ')[0];
        const suffix = (args.suffix || '').trim();
        if (suffix) {
          const pt = M.patterns.find((p) => p.baseCmd === baseName && p.suffix === suffix);
          if (pt) pt.usedCount++; else M.patterns.push({ id: M.nextId++, baseCmd: baseName, suffix, usedCount: 1 });
        }
        sn.usedCount++; sn.lastUsed = TODAY;
        return null;
      }
      case 'explore': {
        const sn = M.snippets.find((s) => s.id === args.id);
        if (!sn) return null;
        const baseName = sn.tool ? (sn.command ? sn.tool + ' ' + sn.command : sn.tool) : sn.cmd.split(' ')[0];
        const items = M.paramTree.filter((p) => p.tool === (sn.tool || '') && (p.command || '') === (sn.command || ''));
        const groups = [];
        for (const key of CATEGORY_ORDER) {
          const its = items.filter((p) => p.category === key);
          if (its.length) groups.push({ key, label: CATEGORY_LABELS[key], items: its.map(cloneSn) });
        }
        // 未知分类兜底（与 Rust 端一致，如 --help 生成的 other）
        for (const cat of [...new Set(items.map((p) => p.category))]) {
          if (CATEGORY_ORDER.includes(cat)) continue;
          groups.push({ key: cat, label: CATEGORY_LABELS[cat] || cat, items: items.filter((p) => p.category === cat).map(cloneSn) });
        }
        const patterns = M.patterns.filter((p) => p.baseCmd === baseName).sort((a, b) => b.usedCount - a.usedCount).slice(0, 6).map(cloneSn);
        let related = M.snippets.filter((s) => s.id !== sn.id && s.tool && s.tool === sn.tool).slice(0, 8);
        if (related.length < 4) {
          const ids = new Set([sn.id, ...related.map((r) => r.id)]);
          related = related.concat(M.snippets.filter((s) => !ids.has(s.id)).slice(0, 8 - related.length));
        }
        return { base: cloneSn(sn), baseName, groups, patterns, related: related.map(cloneSn) };
      }
      case 'explore_save': {
        const base = M.snippets.find((s) => s.id === args.id);
        if (!base) throw new Error('基础记录不存在');
        const p = args.payload;
        const [tool, command] = deriveToolCommand(p.cmd);
        const sn = {
          id: M.nextId++, tool, command, project: base.project,
          intent: p.intent || base.intent + '（组合）', cmd: p.cmd, variantOf: base.id,
          source: 'self', verified: false, usedCount: 1, lastUsed: TODAY, created: TODAY,
          notes: '由发散面板保存的组合',
          tags: Array.from(new Set([...(p.tags || base.tags || []).map((t) => t.toLowerCase()), ...autoTags(p.cmd, p.intent || '')])).sort(),
        };
        M.snippets.unshift(sn);
        return cloneSn(sn);
      }
      case 'stats': {
        const by = { self: 0, tldr: 0, doc: 0 };
        M.snippets.forEach((s) => { by[s.source] = (by[s.source] || 0) + 1; });
        return {
          total: M.snippets.length, self: by.self, tldr: by.tldr, doc: by.doc,
          projects: Array.from(new Set(M.snippets.map((s) => s.project).filter(Boolean))).sort(),
          dataDir: '（浏览器 Mock 模式）',
        };
      }
      case 'tags': {
        const cnt = {};
        M.snippets.forEach((s) => (s.tags || []).forEach((t) => { cnt[t] = (cnt[t] || 0) + 1; }));
        return Object.keys(cnt).map((name) => ({ name, count: cnt[name] })).sort((a, b) => b.count - a.count || a.name.localeCompare(b.name)).slice(0, 60);
      }
      case 'import_tldr':
        return { pages: 0, snippets: 0, skipped: 0 };
      case 'generate_params': {
        // Mock：给当前 tool/command 补一个「生成参数」示例，验证 UI 流程
        const tool = args.tool || '';
        const command = args.command || '';
        const flag = '--from-help';
        const exists = M.paramTree.some((p) => p.tool === tool && (p.command || '') === command && p.flag === flag);
        if (exists) return { parsed: 12, added: 0 };
        M.paramTree.push({ id: M.nextId++, tool, command, category: 'other', flag, desc: '（Mock）由 --help 生成的示例参数', example: flag });
        return { parsed: 12, added: 1 };
      }
      case 'copy_to_clipboard':
        try { await navigator.clipboard.writeText(args.text); } catch (e) { /* 忽略：浏览器权限 */ }
        return null;
      case 'recent_history':
        return M.history.slice(0, (args.limit || 100)).map((e) => ({
          ts: e.ts,
          cmd: e.cmd,
          saved: M.snippets.some((s) => (s.source === 'self' || s.source === 'ai') && s.cmd === e.cmd),
        }));
      case 'llm_suggest': {
        if (M.settings.llm_enabled !== '1') {
          throw new Error('AI 兜底未开启（⚙️ 设置 → AI 兜底 → 勾选启用）');
        }
        // Mock：模拟 AI 生成两条并入库
        const gen = [
          { intent: '（AI）查目录磁盘占用', cmd: 'du -sh ./* | sort -rh | head -10' },
          { intent: '（AI）找出最大的 5 个文件', cmd: 'find . -type f -size +100M -exec ls -lh {} \\; | sort -k5 -rh | head -5' },
        ];
        const out = [];
        for (const g of gen) {
          if (M.snippets.some((s) => s.cmd === g.cmd)) continue;
          const [tool, command] = deriveToolCommand(g.cmd);
          const sn = {
            id: M.nextId++, tool, command, project: null,
            intent: g.intent, cmd: g.cmd, variantOf: null,
            source: 'ai', verified: false, usedCount: 0, lastUsed: null, created: TODAY,
            notes: 'AI 兜底生成，请确认后使用',
            tags: Array.from(new Set(autoTags(g.cmd, g.intent))).sort(),
          };
          M.snippets.unshift(sn);
          out.push(cloneSn(sn));
        }
        return out;
      }
      case 'get_doc_body': {
        const sn = M.snippets.find((s) => s.id === args.id);
        return sn && sn.source === 'doc'
          ? 'TAR(1)  User Commands  TAR(1)\n\nNAME\n       tar - 一个归档工具\n\n（Mock 全文）打包/解包 tar 归档，常配合 gzip。\n\nSYNOPSIS\n       tar [-zxcvf] [文件...]\n'
          : null;
      }
      case 'get_settings':
        return Object.assign({}, M.settings);
      case 'save_settings':
        Object.assign(M.settings, args.values || {});
        return null;
      case 'autostart_status':
        return M.autostart;
      case 'autostart_enable':
        M.autostart = true; return null;
      case 'autostart_disable':
        M.autostart = false; return null;
      default:
        throw new Error('Mock 未实现命令: ' + cmd);
    }
  }

  window.SNIP = {
    invoke,
    isTauri: !!tauriCore,
  };
})();
