'use strict';
'require rpc';
'require ui';

// 注意：LuCI 的 `expect` 不是字段白名单 —— 它只取【第一个键】的值，
// 且类型不匹配时直接返回默认值（见 luci-static/resources/rpc.js handleCallReply）。
// 所以这里要拿整个返回对象时，必须**不写 expect**（或写 `expect: {}`）。
var callStatus = rpc.declare({
	object: 'luci.autheap_client',
	method: 'status'
});

// 专用的文件列表接口（字段名兼容 log_files / files 两种后端）
var callLogFiles = rpc.declare({
	object: 'luci.autheap_client',
	method: 'log_files'
});

var callLog = rpc.declare({
	object: 'luci.autheap_client',
	method: 'log',
	params: [ 'offset', 'file', 'mode', 'limit' ]
});

var callLogClear = rpc.declare({
	object: 'luci.autheap_client',
	method: 'log_clear'
});

var REFRESH_MS    = 1500;    // 增量拉取间隔
var HISTORY_LIMIT = 131072;  // “加载完整历史”最多取 128KB
var LOG_KEEP      = 4000;    // 内存中保留的最大行数
var LOG_SHOW      = 800;     // 界面显示的最大行数

// 若报 “Access denied”，通常是升级了 luci 包但 ACL 还没生效（rpcd 未重启 / 会话缓存旧 ACL）
var ACL_HINT = '（若提示 Access denied：请在设备上执行 /etc/init.d/rpcd restart 并重新登录 LuCI，再 Ctrl+F5 强刷）';

function hhmmss() {
	var d = new Date();
	function p(n) { return (n < 10 ? '0' : '') + n; }
	return p(d.getHours()) + ':' + p(d.getMinutes()) + ':' + p(d.getSeconds());
}

function kb(n) {
	n = Number(n) || 0;
	return n < 1024 ? (n + ' B') : ((n / 1024).toFixed(1) + ' KB');
}

function err(e) { return (e && (e.message || e.toString())) || '未知错误'; }

// 取日志文件列表：优先专用接口；若该接口不可用（例如 ACL 尚未生效、rpcd 未重启、
// 设备上还是旧的 luci 包），自动回退到一直有授权的 status.log_files，避免整页报错。
function pickFiles(r) {
	r = r || {};
	return r.log_files || r.files || [];
}

return L.view.extend({
	load: function () {
		// status 是「永远有 ACL 授权」的方法，用它做基石：
		// 即使 log / log_files 因 ACL 未刷新（未重启 rpcd / 未重新登录）打不开，
		// 页面也要能渲染出来，并在页内提示原因。
		return callStatus().then(function (st) {
			return callLogFiles().then(function (r) {
				return [ st || {}, pickFiles(r), '' ];
			}, function () {
				return [ st || {}, pickFiles(st), '' ];
			});
		}, function (e) {
			return [ {}, [], '无法调用后端 status：' + err(e) + ACL_HINT ];
		});
	},

	render: function (data) {
		var st = (data && data[0]) || {};
		var files0 = (data && data[1]) || pickFiles(st);
		var loadErr = (data && data[2]) || '';

		// ---- 状态行元素 ----
		var elFiles = E('select', { 'class': 'cbi-input-select' });
		var elMeta = E('em', { 'style': 'font-size:12px;margin-left:8px' }, '尚未刷新');
		var elErr = E('div', { 'style': 'color:#c62828;font-size:12px' });
		var elLog = E('pre', {
			'style': 'max-height:520px;overflow:auto;background:#1e1e1e;color:#9f9;padding:8px;' +
			         'font-size:12px;line-height:1.35;white-space:pre-wrap;word-break:break-all'
		}, '(暂无日志)');
		var autoChk = E('input', { 'type': 'checkbox', 'checked': true });

		// ---- 视图状态 ----
		var st8 = {
			file: '',      // '' = 当前日志；'1'..'9' = 历史 .1/.2…
			offset: 0,
			lines: [],
			tail: '',
			running: false,
			gen: 0
		};

		function addNotice(text) {
			st8.lines = st8.lines.concat([ text ]);
			if (st8.lines.length > LOG_KEEP)
				st8.lines = st8.lines.slice(-LOG_KEEP);
		}

		function renderLog() {
			var out = st8.lines.slice(-LOG_SHOW).reverse().join('\n');
			elLog.textContent = out || '(暂无日志)';
		}

		function pushChunk(text) {
			var parts = String(text || '').split('\n');
			parts[0] = st8.tail + parts[0];
			st8.tail = parts.pop();
			if (parts.length) {
				st8.lines = st8.lines.concat(parts);
				if (st8.lines.length > LOG_KEEP)
					st8.lines = st8.lines.slice(-LOG_KEEP);
				renderLog();
			}
		}

		function applyHistory(r) {
			r = r || {};
			st8.lines = [];
			st8.tail = '';
			if (typeof r.offset === 'number')
				st8.offset = r.offset;
			if (r.data)
				pushChunk(r.data + '\n');
			renderLog();
		}

		// ---- 文件列表 ----
		function rebuildFiles(files) {
			files = files || [];
			var cur = st8.file;
			while (elFiles.firstChild) elFiles.removeChild(elFiles.firstChild);
			files.forEach(function (f) {
				elFiles.appendChild(E('option', { 'value': f.id || '' },
					(f.id ? ('历史 ' + f.name) : ('当前 ' + f.name)) + '（' + kb(f.size) + '）'));
			});
			if (!files.length)
				elFiles.appendChild(E('option', { 'value': '' },
					'(暂无日志文件：' + (st.log_file || '未知路径') + ')'));
			// 用属性赋值选中项（避免依赖 E() 对 null 属性的处理）
			elFiles.value = cur;
		}

		function refreshFileList() {
			return callLogFiles().then(function (r) {
				rebuildFiles(pickFiles(r));
			}, function () {
				return callStatus().then(function (s) {
					rebuildFiles(pickFiles(s));
				}, function () {});
			});
		}

		// log 接口不可用时的兜底：status 一直有 ACL 授权，且它自带最多 2KB 的日志快照
		// （字段 log）。这样即使 ACL 未刷新，“日志”页也能看到内容而不是空白。
		function statusSnapshot(e) {
			return callStatus().then(function (s) {
				s = s || {};
				if (s.log) {
					st8.lines = String(s.log).split('\n');
					st8.tail = '';
					renderLog();
					addNotice('---- log 接口不可用（' + err(e) + '），已改用 status 日志快照 ----');
					addNotice(ACL_HINT);
				}
				throw e;
			}, function () { throw e; });
		}

		// ---- 拉取 ----
		// 注意：callLog 声明了 params，必须**按位置**传参（传对象会把整个对象
		// 塞进第一个参数 offset，后端就收不到正确的 file/mode/limit）。
		function loadHistory(file) {
			return callLog(0, file, 'history', HISTORY_LIMIT)
				.then(function (r) { applyHistory(r); }, function (e) { return statusSnapshot(e); });
		}

		function fetchTail() {
			return callLog(st8.offset, st8.file, 'tail', 65536)
				.then(function (r) {
					r = r || {};
					if (typeof r.size === 'number' && r.size < st8.offset) {
						// 文件被轮转/清空：重置缓冲后重新加载历史，并给出提示
						addNotice('---- 日志已轮转/清空（' + hhmmss() + '），已重新加载 ----');
						return loadHistory(st8.file).then(function () { renderLog(); });
					}
					if (typeof r.offset === 'number')
						st8.offset = r.offset;
					if (r.data)
						pushChunk(r.data);
					return r;
				}, function (e) { return statusSnapshot(e); });
		}

		// ---- 自动刷新循环（自排队，避免并发堆积）----
		var timer = null;

		function schedule() {
			if (timer) { window.clearTimeout(timer); timer = null; }
			if (!autoChk.checked || document.hidden || st8.file !== '')
				return;
			timer = window.setTimeout(tick, REFRESH_MS);
		}

		function tick() {
			var my = st8.gen;
			var pend = 2;
			var errs = [];

			function done() {
				if (--pend > 0) return;
				if (my !== st8.gen) return;
				elMeta.textContent = '最后更新 ' + hhmmss() +
					'  ·  已读 ' + kb(st8.offset) + '  ·  保留 ' + st8.lines.length + ' 行' +
					(st8.file === '' ? '' : '（历史文件，自动刷新已暂停）');
				elErr.textContent = errs.join(' | ');
				schedule();
			}

			fetchTail().then(function () {}, function (e) {
				if (my === st8.gen) errs.push('日志刷新失败: ' + err(e) +
					'（若刚升级，请重新登录 LuCI 并 Ctrl+F5 强制刷新）');
			}).then(done);

			callStatus().then(function (s) {
				st = s || {};
			}, function () {}).then(done);
		}

		function reload(keepOffset) {
			st8.gen++;
			if (!keepOffset) st8.offset = 0;
			return refreshFileList().then(function () {
				return loadHistory(st8.file);
			});
		}

		// ---- 事件 ----
		elFiles.addEventListener('change', function () {
			st8.file = elFiles.value || '';
			reload(false).then(schedule);
		});

		autoChk.addEventListener('change', function () {
			if (autoChk.checked) { st8.gen++; tick(); } else schedule();
		});

		window.addEventListener('beforeunload', function () {
			st8.gen++;
			if (timer) window.clearTimeout(timer);
		});

		function btn(label, cls, fn) {
			return E('button', { 'class': 'cbi-button ' + cls, click: fn }, label);
		}

		var section = E('div', { 'class': 'cbi-section' }, [
			E('div', { 'style': 'display:flex;align-items:center;gap:10px;flex-wrap:wrap' }, [
				E('label', {}, [ autoChk, ' 自动刷新（实时，1.5 秒）' ]),
				btn('立即刷新', 'cbi-button-action', function () { st8.gen++; tick(); }),
				btn('加载完整历史', 'cbi-button-neutral', function () {
					st8.gen++;
					loadHistory(st8.file).then(function () { elMeta.textContent = '已加载历史'; });
				}),
				btn('跳到最新', 'cbi-button-neutral', function () {
					st8.gen++;
					st8.file = '';
					elFiles.value = '';
					reload(false).then(schedule);
				}),
				btn('清空日志', 'cbi-button-reset', function () {
					if (!confirm('确定清空当前与全部历史日志？')) return;
					callLogClear().then(function () { return reload(false); }).then(schedule)
						.catch(function (e) {
							elErr.textContent = '清空失败: ' + err(e) + ACL_HINT;
						});
				})
			]),
			E('div', { 'style': 'margin:8px 0;display:flex;align-items:center;gap:8px;flex-wrap:wrap' }, [
				E('strong', {}, '日志文件:'),
				elFiles,
				elMeta
			]),
			elErr,
			elLog
		]);

		st8.gen++;
		rebuildFiles(files0);
		if (loadErr)
			elErr.textContent = loadErr;
		if (!st.running && !loadErr) {
			// 服务没跑时后端不会写新日志（已有文件仍可读）——这种情况最常见
			addNotice('---- 后端服务当前未运行（日志可能为空）：' +
				'/etc/init.d/autheap-client start 或者到状态页点“启动” ----');
		} else if (st.running && !(files0 || []).length) {
			// 进程在跑但日志文件不存在：说明这个进程不是由本版 init.d 启动的
			// （旧版启动脚本/手动命令行启动不会创建日志文件）
			addNotice('---- 服务在运行，但日志文件不存在：' + (st.log_file || '未知路径') +
				'。说明当前进程不是本版 init.d 启动的（手动命令行启动/旧启动脚本）；' +
				'执行 /etc/init.d/autheap-client restart 后即可看到日志 ----');
		}
		loadHistory('').then(function () {
			tick();
		});

		return E('div', { 'class': 'cbi-map' }, [
			E('h2', {}, 'AuthEAP-Client 日志'),
			E('div', { 'class': 'cbi-map-descr' },
				'实时查看当前日志，并可查看历史日志（自动轮转清理后保留的 .1/.2…）。' +
				'轮转/清理策略在“配置”页调整（log_max_kb / log_keep / log_rotate_secs）。'),
			section
		]);
	}
});
