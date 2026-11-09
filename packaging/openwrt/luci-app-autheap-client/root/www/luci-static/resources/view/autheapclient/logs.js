'use strict';

'require rpc';
'require ui';
'require view';

// 注意：rpc.declare 的 `expect` 只取第一个 key；这里需要完整返回对象，故省略。
var callStatus = rpc.declare({
	object: 'luci.autheap_client',
	method: 'status'
});

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

var REFRESH_MS = 1500;    // 增量拉取间隔
var HISTORY_LIMIT = 131072; // “加载完整历史”最多取 128KB
var LOG_KEEP = 4000;      // 内存中保留的最大行数
var LOG_SHOW = 800;       // 界面显示的最大行数

function hhmmss() {
	var d = new Date();
	function p(n) { return (n < 10 ? '0' : '') + n; }
	return p(d.getHours()) + ':' + p(d.getMinutes()) + ':' + p(d.getSeconds());
}

function kb(n) {
	n = Number(n) || 0;
	return n < 1024 ? (n + ' B') : ((n / 1024).toFixed(1) + ' KB');
}

function err(e) {
	return (e && (e.message || e.toString())) || '未知错误';
}

function pickFiles(r) {
	r = r || {};
	return r.log_files || r.files || [];
}

function viewStyle() {
	return E('style', {}, [
		'.autheap-log {',
		'  font-family: var(--font-monospace, monospace);',
		'  font-size: 12px;',
		'  line-height: 1.45;',
		'  white-space: pre-wrap;',
		'  word-break: break-all;',
		'  max-height: 60vh;',
		'  overflow: auto;',
		'  padding: 8px 10px;',
		'  margin: 0;',
		'  background: var(--background-color-high, rgba(127,127,127,.08));',
		'  color: var(--text-color-high, inherit);',
		'  border: 1px solid var(--border-color-medium, rgba(127,127,127,.35));',
		'  border-radius: var(--border-radius, 4px);',
		'}',
		'.autheap-log .notice { opacity: .75; }',
		'.autheap-inline { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }'
	].join('\n'));
}

function row(title, value) {
	return E('div', { 'class': 'cbi-value' }, [
		E('label', { 'class': 'cbi-value-title' }, title),
		E('div', { 'class': 'cbi-value-field' }, value)
	]);
}

return view.extend({
	load: function () {
		// status 是“永远有 ACL 授权”的方法，用它做基石：
		// 即使 log / log_files 因 ACL 未刷新打不开，页面也能渲染并提示原因。
		return callStatus().then(function (st) {
			return callLogFiles().then(function (r) {
				return [ st || {}, pickFiles(r), '' ];
			}, function () {
				return [ st || {}, pickFiles(st), '' ];
			});
		}, function (e) {
			return [ {}, [], _('无法调用后端 status：') + err(e) ];
		});
	},

	render: function (data) {
		var st = (data && data[0]) || {};
		var files0 = (data && data[1]) || pickFiles(st);
		var loadErr = (data && data[2]) || '';

		var elFiles = E('select', { 'class': 'cbi-input-select' });
		var elMeta = E('em', { 'class': 'cbi-value-description' }, _('尚未刷新'));
		var elErr = E('div', { 'class': 'alert-message error', 'style': 'display:none' });
		var elLog = E('pre', { 'class': 'autheap-log' }, _('（暂无日志）'));
		var autoChk = E('input', { 'type': 'checkbox', 'checked': true });

		var st8 = {
			file: '',
			offset: 0,
			lines: [],
			tail: '',
			running: !!st.running,
			gen: 0
		};

		function showError(msg) {
			elErr.textContent = msg || '';
			elErr.style.display = msg ? '' : 'none';
		}

		function addNotice(text) {
			st8.lines = st8.lines.concat([ text ]);
			if (st8.lines.length > LOG_KEEP)
				st8.lines = st8.lines.slice(-LOG_KEEP);
		}

		function renderLog() {
			var out = st8.lines.slice(-LOG_SHOW).reverse().join('\n');
			elLog.textContent = out || _('（暂无日志）');
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

		function rebuildFiles(files) {
			files = files || [];
			var cur = st8.file;
			while (elFiles.firstChild)
				elFiles.removeChild(elFiles.firstChild);

			var hasCurrent = false;
			for (var j = 0; j < files.length; j++)
				if ((files[j].id || '') === '')
					hasCurrent = true;

			// 当前日志文件不存在时也保留“当前”选项，避免下拉框默认跳到某个历史文件，
			// 而内部状态仍停留在当前文件（两边不一致）。
			if (!hasCurrent && files.length)
				elFiles.appendChild(E('option', { 'value': '' },
					_('当前日志（文件不存在：') + (st.log_file || _('未知路径')) + '）'));

			files.forEach(function (f) {
				var id = String(f.id || '');
				var prefix = id.indexOf('line:') === 0 ? _('线路 ') : (id ? _('历史 ') : _('当前 '));
				elFiles.appendChild(E('option', { 'value': id },
					prefix + f.name + '（' + kb(f.size) + '）'));
			});

			if (!files.length) {
				elFiles.appendChild(E('option', { 'value': '' },
					_('（暂无日志文件：') + (st.log_file || _('未知路径')) + '）'));
				elFiles.disabled = true;
				elFiles.value = '';
			}
			else {
				elFiles.disabled = false;
				var found = false;
				for (var j = 0; j < files.length; j++)
					if ((files[j].id || '') === cur)
						found = true;
				var chosen = (found || cur === '') ? cur : (files[0].id || '');
				elFiles.value = chosen;
				// 选中的历史文件被清空/轮转掉时，内部状态也要跟着切回实际选中的文件。
				if (chosen !== cur)
					st8.file = chosen;
			}
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

		// log 接口不可用时的兜底：status 一直有 ACL 授权，且它自带最多 2KB 的日志快照。
		function statusSnapshot(e) {
			return callStatus().then(function (s) {
				s = s || {};
				if (s.log) {
					st8.lines = String(s.log).split('\n');
					st8.tail = '';
					addNotice(_('---- log 接口不可用（') + err(e) + _('），已改用 status 日志快照 ----'));
					addNotice(_('---- 若刚升级 LuCI 包，请执行 /etc/init.d/rpcd restart 并重新登录 ----'));
					renderLog();
				}
				throw e;
			}, function () { throw e; });
		}

		function loadHistory(file) {
			return callLog(0, file, 'history', HISTORY_LIMIT)
				.then(function (r) { applyHistory(r); }, function (e) { return statusSnapshot(e); });
		}

		function fetchTail() {
			return callLog(st8.offset, st8.file, 'tail', 65536)
				.then(function (r) {
					r = r || {};
					if (typeof r.size === 'number' && r.size < st8.offset) {
						return loadHistory(st8.file).then(function () {
							addNotice(_('---- 日志已轮转/清空（') + hhmmss() + _('），已重新加载 ----'));
							renderLog();
						});
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
				elMeta.textContent = _('最后更新 ') + hhmmss() +
					' · ' + _('已读 ') + kb(st8.offset) + ' · ' + _('保留 ') + st8.lines.length + _(' 行') +
					(st8.file === '' ? '' : _('（历史文件，自动刷新已暂停）'));
				showError(errs.join(' | '));
				schedule();
			}

			fetchTail().then(function () {}, function (e) {
				if (my === st8.gen)
					errs.push(_('日志刷新失败：') + err(e));
			}).then(done);

			callStatus().then(function (s) {
				s = s || {};
				var logPathChanged = (st.log_file && s.log_file && st.log_file !== s.log_file);
				st = s;
				st8.running = !!st.running;
				// 配置页改了 log_file 后，日志页无需重新打开也应刷新文件列表。
				if (logPathChanged)
					return refreshFileList();
			}, function () {}).then(done);
		}

		function reload(keepOffset) {
			st8.gen++;
			if (!keepOffset)
				st8.offset = 0;
			return refreshFileList().then(function () {
				return loadHistory(st8.file);
			}).catch(function (e) {
				showError(_('加载日志失败：') + err(e));
			});
		}

		elFiles.addEventListener('change', function () {
			st8.file = elFiles.value || '';
			reload(false).then(schedule);
		});

		autoChk.addEventListener('change', function () {
			if (autoChk.checked) { st8.gen++; tick(); }
			else schedule();
		});

		document.addEventListener('visibilitychange', function () {
			if (!document.hidden && autoChk.checked && st8.file === '') {
				if (timer) { window.clearTimeout(timer); timer = null; }
				st8.gen++;
				tick();
			}
		});

		window.addEventListener('beforeunload', function () {
			st8.gen++;
			if (timer) window.clearTimeout(timer);
		});

		function btn(label, cls, fn) {
			return E('button', { 'class': 'cbi-button ' + cls, 'click': fn }, label);
		}

		function clearLogs() {
			ui.showModal(_('清空日志'), [
				E('p', {}, _('确定清空当前日志与全部历史日志？此操作不可撤销。')),
				E('div', { 'class': 'right' }, [
					E('button', {
						'class': 'cbi-button',
						'click': ui.hideModal
					}, _('取消')),
					E('button', {
						'class': 'cbi-button cbi-button-negative',
						'click': function () {
							ui.hideModal();
							callLogClear().then(function () {
								return reload(false);
							}).then(schedule).catch(function (e) {
								showError(_('清空失败：') + err(e));
							});
						}
					}, _('清空'))
				])
			]);
		}

		var section = E('div', { 'class': 'cbi-section' }, [
			E('h3', {}, _('日志文件')),
			E('div', { 'class': 'cbi-section-node' }, [
				row(_('当前文件'), E('div', { 'class': 'autheap-inline' }, [
					elFiles,
					autoChk,
					E('label', {}, _('自动刷新')),
					E('button', {
						'class': 'cbi-button cbi-button-action',
						'click': function () { st8.gen++; tick(); }
					}, _('立即刷新')),
					E('button', {
						'class': 'cbi-button cbi-button-neutral',
						'click': function () {
							st8.gen++;
							loadHistory(st8.file).then(function () {
								elMeta.textContent = _('已加载历史');
							}, function (e) {
								showError(_('加载历史失败：') + err(e));
							});
						}
					}, _('加载完整历史')),
					E('button', {
						'class': 'cbi-button cbi-button-neutral',
						'click': function () {
							st8.gen++;
							st8.file = '';
							elFiles.value = '';
							reload(false).then(schedule);
						}
					}, _('跳到最新')),
					E('button', {
						'class': 'cbi-button cbi-button-reset',
						'click': clearLogs
					}, _('清空日志')),
					elMeta
				])),
				elErr,
				row(_('日志内容'), elLog)
			])
		]);

		st8.gen++;
		rebuildFiles(files0);
		if (loadErr)
			showError(loadErr);

		// 先加载日志内容，再追加提示行；否则 loadHistory 会清空刚加的提示。
		loadHistory('').then(function () {
			if (!st.running && !loadErr) {
				addNotice(_('---- 后端服务当前未运行（日志可能为空）：/etc/init.d/autheap-client start ----'));
			}
			else if (st.running && !(files0 || []).length) {
				addNotice(_('---- 服务在运行，但日志文件不存在：') + (st.log_file || _('未知路径')) +
					_('。执行 /etc/init.d/autheap-client restart 后即可看到日志 ----'));
			}
			renderLog();
		}).catch(function (e) {
			showError(_('读取日志失败：') + err(e));
		}).then(function () {
			tick();
		});

		return E('div', { 'class': 'cbi-map' }, [
			viewStyle(),
			E('h2', {}, _('AuthEAP-Client 日志')),
			E('div', { 'class': 'cbi-map-descr' },
				_('实时查看当前日志，并可查看历史日志（自动轮转清理后保留的 .1/.2…）。' +
				  '轮转/清理策略在“配置 → 日志”中调整。')),
			section
		]);
	}
});
