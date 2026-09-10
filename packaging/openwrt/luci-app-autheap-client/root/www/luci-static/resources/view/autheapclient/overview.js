'use strict';
'require rpc';

var callStatus = rpc.declare({
	object: 'luci.autheap_client',
	method: 'status',
	expect: {}
});

// 增量读取日志：只取上次 offset 之后的新增字节
var callLog = rpc.declare({
	object: 'luci.autheap_client',
	method: 'log',
	params: [ 'offset' ],
	expect: { offset: 0, size: 0, data: '' }
});

var callControl = rpc.declare({
	object: 'luci.autheap_client',
	method: 'control',
	expect: { res: true }
});

var REFRESH_MS = 1500;   // 拉取间隔
var LOG_KEEP   = 400;    // 内存中保留的最大行数
var LOG_SHOW   = 80;     // 界面上显示的行数

function row(label, el) {
	return E('div', { 'class': 'cbi-value' }, [
		E('div', { 'class': 'cbi-value-title' }, label),
		E('div', { 'class': 'cbi-value-field' }, [ el ])
	]);
}

function esc(s) {
	return String(s)
		.replace(/&/g, '&amp;')
		.replace(/</g, '&lt;')
		.replace(/>/g, '&gt;')
		.replace(/"/g, '&quot;');
}

function hhmmss() {
	var d = new Date();
	function p(n) { return (n < 10 ? '0' : '') + n; }
	return p(d.getHours()) + ':' + p(d.getMinutes()) + ':' + p(d.getSeconds());
}

function fmtPlugins(p) {
	if (!p) return '-';
	var parts = [];
	if (p.rjv3) parts.push('rjv3');
	if (p.printer) parts.push('printer');
	if (p.stats) parts.push('stats');
	if (p.extra) {
		String(p.extra).split(/[\s,]+/).forEach(function (n) { if (n) parts.push(n); });
	}
	return parts.length ? parts.join(', ') : '（无）';
}

return L.view.extend({
	load: function () {
		return callStatus();
	},

	render: function (st) {
		var self = this;

		// ---- 可实时更新的元素 ----
		var elAuth = E('div', {
			'style': 'padding:10px 14px;margin-bottom:10px;border:1px solid #888;' +
			         'border-radius:4px;background:rgba(127,127,127,.12);' +
			         'font-size:20px;font-weight:bold;text-align:center'
		});
		var elBoot = E('strong', {});
		var elRun  = E('strong', {});
		var elIfname = E('strong', {});
		var elUser = E('strong', {});
		var elPwd = E('strong', {});
		var elPlug = E('strong', {});
		var elVer = E('strong', {});
		var elCore = E('strong', {});
		var elMeta = E('em', {}, '尚未刷新');
		var elErr  = E('div', { 'style': 'color:#c62828;font-size:12px' });
		var elLog = E('pre', {
			'style': 'max-height:340px;overflow:auto;background:#222;color:#9f9;padding:8px;margin-top:4px'
		}, '(暂无日志)');
		var bootBtn = E('button', { 'class': 'cbi-button cbi-button-action' });
		var autoChk = E('input', { 'type': 'checkbox', 'checked': true });
		var autoLbl = E('label', {}, [ autoChk, ' 自动刷新（实时）' ]);

		// ---- 日志缓冲（旧 → 新）----
		var logLines = [];
		var logTail  = '';     // 尚未收到换行的半行

		function renderLog() {
			var out = logLines.slice(-LOG_SHOW).reverse().join('\n');
			elLog.textContent = out || '(暂无日志)';
		}

		function pushChunk(text) {
			var parts = String(text || '').split('\n');
			parts[0] = logTail + parts[0];
			logTail = parts.pop();
			if (parts.length) {
				logLines = logLines.concat(parts);
				if (logLines.length > LOG_KEEP)
					logLines = logLines.slice(-LOG_KEEP);
				renderLog();
			}
		}

		function applyStatus(s) {
			var boot = (s.enabled === true || s.enabled === 1);
			var running = (s.running === true || s.running === 1);
			var text, color;

			if (!running) {
				text = '未运行'; color = '#888888';
			} else if (s.auth === 'success') {
				text = '认证成功'; color = '#2e7d32';
			} else if (s.auth === 'failed') {
				text = '认证失败'; color = '#c62828';
			} else {
				text = '认证中…'; color = '#ef8f00';
			}
			elAuth.textContent = text;
			elAuth.style.color = color;
			elAuth.style.borderColor = color;

			elBoot.textContent = boot ? '是' : '否';
			elRun.textContent = running ? '是' : '否';
			elIfname.textContent = s.ifname || '-';
			elUser.textContent = s.username || '-';
			elPwd.textContent = (typeof s.pwdlen === 'number' ? s.pwdlen : '-') + ' 位';			elPlug.textContent = fmtPlugins(s.plugins);
			elVer.textContent = s.version || '-';
			elCore.textContent = s.core || '-';

			bootBtn.textContent = boot ? '关闭开机自启' : '开启开机自启';
			bootBtn.setAttribute('data-boot', boot ? '1' : '0');
		}

		function control(action) {
			return callControl({ action: action })
				.then(function () { return callStatus(); })
				.then(function (s) { applyStatus(s); })
				.catch(function (e) { elErr.textContent = '操作失败: ' + (e.message || e); });
		}

		function ctrlButton(label, action) {
			return E('button', {
				'class': 'cbi-button cbi-button-action',
				click: function () { control(action); }
			}, label);
		}

		// ---- 刷新循环：自排队 setTimeout，绝不并发堆积 ----
		var timer = null;
		var logOffset = 0;
		var lastStatus = {};
		var gen = 0;             // 页码（视图重建后旧循环自动失效）

		function err(e) { return (e && (e.message || e.toString())) || '未知错误'; }

		function schedule() {
			if (timer) { window.clearTimeout(timer); timer = null; }
			if (!autoChk.checked || document.hidden) return;
			timer = window.setTimeout(tick, REFRESH_MS);
		}

		function tick() {
			var my = gen;
			var errs = [];
			var pend = 2;

			function done() {
				if (--pend > 0) return;
				if (my !== gen) return;
				elMeta.textContent = '最后更新 ' + hhmmss();
				elErr.textContent = errs.join(' | ');
				schedule();
			}

			callStatus().then(function (s) {
				if (my !== gen) return;
				lastStatus = s || {};
				applyStatus(lastStatus);
			}, function (e) {
				if (my === gen) errs.push('状态刷新失败: ' + err(e));
			}).then(done);

			callLog({ offset: logOffset }).then(function (r) {
				if (my !== gen) return;
				r = r || {};
				if (typeof r.offset === 'number') logOffset = r.offset;
				if (r.data) pushChunk(r.data);
			}, function (e) {
				if (my !== gen) return;
				errs.push('日志刷新失败: ' + err(e) +
					'（若刚装完插件，请重新登录 LuCI 并 Ctrl+F5 强制刷新）');
				// 回退：后端仍是旧版时，用 status 里的兼容字段
				if (!logLines.length && typeof lastStatus.log === 'string' && lastStatus.log) {
					logLines = lastStatus.log.split('\n');
					logTail = '';
					renderLog();
				}
			}).then(done);
		}

		autoChk.addEventListener('change', function () {
			if (autoChk.checked) { gen++; tick(); } else schedule();
		});

		window.addEventListener('beforeunload', function () {
			gen++;
			if (timer) window.clearTimeout(timer);
		});

		applyStatus(st || {});
		// 首次进入：offset=0 → 后端只回最近一段日志
		gen++;
		tick();

		bootBtn.addEventListener('click', function () {
			control(bootBtn.getAttribute('data-boot') === '1' ? 'disable' : 'enable');
		});

		var section = E('div', { 'class': 'cbi-section' }, [
			elAuth,
			row('开机自启', elBoot),
			row('运行中', elRun),
			row('接口', elIfname),
			row('用户名', elUser),
			row('密码长度', elPwd),			row('插件', elPlug),
			row('版本', elVer),
			row('核心', elCore),
			E('div', { 'class': 'cbi-page-actions' }, [
				ctrlButton('启动', 'start'),
				ctrlButton('停止', 'stop'),
				ctrlButton('重启', 'restart'),
				bootBtn
			]),
			E('h3', {}, '最近日志（流式，最新在上）'),
			E('div', { 'style': 'display:flex;align-items:center;gap:12px;font-size:12px' }, [
				autoLbl,
				E('button', {
					'class': 'cbi-button cbi-button-neutral',
					click: function () { gen++; logOffset = 0; logLines = []; logTail = ''; tick(); }
				}, '立即刷新'),
				elMeta
			]),
			elErr,
			elLog
		]);

		return E('div', { 'class': 'cbi-map' }, [
			E('h2', {}, 'AuthEAP-Client'),
			E('div', { 'class': 'cbi-map-descr' }, '认证状态、服务控制与实时日志（增量流式拉取，每 1.5 秒）。'),
			section
		]);
	}
});
