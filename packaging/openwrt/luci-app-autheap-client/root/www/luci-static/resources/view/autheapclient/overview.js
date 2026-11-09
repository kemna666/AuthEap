'use strict';

'require rpc';
'require ui';
'require view';

// 注意：rpc.declare 的 `expect` 不是字段白名单，只取第一个 key；
// 这里需要完整返回对象，因此不写 expect。声明 params 才会按位置映射参数。
var callStatus = rpc.declare({
	object: 'luci.autheap_client',
	method: 'status'
});

var callControl = rpc.declare({
	object: 'luci.autheap_client',
	method: 'control',
	params: [ 'action' ]
});

var callNetTest = rpc.declare({
	object: 'luci.autheap_client',
	method: 'nettest',
	params: [ 'host', 'ifname' ]
});

var REFRESH_MS = 2000;

function truthy(v) {
	return v === true || v === 1 || v === '1' || v === 'true' || v === 'on';
}

function hhmmss() {
	var d = new Date();
	function p(n) { return (n < 10 ? '0' : '') + n; }
	return p(d.getHours()) + ':' + p(d.getMinutes()) + ':' + p(d.getSeconds());
}

function err(e) {
	return (e && (e.message || e.toString())) || '未知错误';
}

function asList(v) {
	if (v == null || v === '')
		return [];
	if (Array.isArray(v))
		return v.map(String);

	return String(v).split(/[\s,]+/).filter(function (n) { return n; });
}

function fmtPlugins(p) {
	if (!p)
		return _('（无）');

	var parts = [];
	if (truthy(p.rjv3)) parts.push('rjv3');
	if (truthy(p.printer)) parts.push('printer');
	if (truthy(p.stats)) parts.push('stats');
	if (truthy(p.beacon)) parts.push('beacon');
	asList(p.extra).forEach(function (n) { parts.push(n); });

	return parts.length ? parts.join('、') : _('（无）');
}

function row(title, value, description) {
	return E('div', { 'class': 'cbi-value' }, [
		E('label', { 'class': 'cbi-value-title' }, title),
		E('div', { 'class': 'cbi-value-field' }, [
			value,
			description ? E('div', { 'class': 'cbi-value-description' }, description) : ''
		])
	]);
}

function viewStyle() {
	return E('style', {}, [
		'.autheap-log {',
		'  font-family: var(--font-monospace, monospace);',
		'  font-size: 12px;',
		'  line-height: 1.45;',
		'  white-space: pre-wrap;',
		'  word-break: break-all;',
		'  max-height: 420px;',
		'  overflow: auto;',
		'  padding: 8px 10px;',
		'  margin: 0;',
		'  background: var(--background-color-high, rgba(127,127,127,.08));',
		'  color: var(--text-color-high, inherit);',
		'  border: 1px solid var(--border-color-medium, rgba(127,127,127,.35));',
		'  border-radius: var(--border-radius, 4px);',
		'}',
		'.autheap-status { font-weight: bold; }',
		'.autheap-inline { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }'
	].join('\n'));
}

return view.extend({
	load: function () {
		return callStatus();
	},

	render: function (st) {
		var self = this;
		var state = st || {};

		var elAuth = E('span', { 'class': 'label autheap-status' }, _('未知'));
		var elBoot = E('strong', {}, _('-'));
		var elBootDelay = E('strong', {}, _('-'));
		var elRun = E('strong', {}, _('-'));
		var elIfname = E('strong', {}, _('-'));
		var elUser = E('strong', {}, _('-'));
		var elPwd = E('strong', {}, _('-'));
		var elPlug = E('strong', {}, _('-'));
		var elBeacon = E('strong', {}, _('-'));
		var elVer = E('strong', {}, _('-'));
		var elCore = E('strong', {}, _('-'));
		var elMeta = E('em', { 'class': 'cbi-value-description' }, _('尚未刷新'));
		var elErr = E('div', { 'class': 'alert-message error', 'style': 'display:none' });
		var elStale = E('div', { 'class': 'alert-message warning', 'style': 'display:none' });

		var autoChk = E('input', { 'type': 'checkbox', 'checked': true });
		var bootBtn = E('button', { 'class': 'cbi-button cbi-button-neutral' }, _('开机自启'));

		function showError(msg) {
			elErr.textContent = msg || '';
			elErr.style.display = msg ? '' : 'none';
		}

		function applyStatus(s) {
			state = s || {};
			var boot = truthy(state.enabled);
			var running = truthy(state.running);
			var auth = String(state.auth || 'unknown');

			if (!running) {
				elAuth.textContent = _('未运行');
				elAuth.className = 'label autheap-status';
			}
			else if (auth === 'waiting') {
				elAuth.textContent = _('等待开机延迟');
				elAuth.className = 'label warning autheap-status';
			}
			else if (auth === 'success') {
				elAuth.textContent = _('认证成功');
				elAuth.className = 'label success autheap-status';
			}
			else if (auth === 'failed') {
				elAuth.textContent = _('认证失败');
				elAuth.className = 'label important autheap-status';
			}
			else {
				elAuth.textContent = _('认证中…');
				elAuth.className = 'label warning autheap-status';
			}

			elBoot.textContent = boot ? _('是') : _('否');
			elBootDelay.textContent = (typeof state.boot_delay === 'number' ? state.boot_delay : 600) + _(' 秒');
			elRun.textContent = running ? _('是') : _('否');
			elIfname.textContent = state.ifname || '-';
			elUser.textContent = state.username || '-';
			elPwd.textContent = (typeof state.pwdlen === 'number' ? state.pwdlen : '-') + _(' 位');
			elPlug.textContent = fmtPlugins(state.plugins);
			elBeacon.textContent = (state.beacon && state.beacon.dst)
				? (state.beacon.dst + ':' + (state.beacon.port || 8083) +
				   _('，每 ') + (state.beacon.interval || 0) + _(' 秒'))
				: _('未启用');
			elVer.textContent = state.version || '-';
			elCore.textContent = state.core || '-';

			bootBtn.textContent = boot ? _('关闭开机自启') : _('开启开机自启');
			bootBtn.setAttribute('data-boot', boot ? '1' : '0');

			if (!running)
				startBtn.textContent = _('启动');
			else if (auth === 'waiting')
				startBtn.textContent = _('立即认证');
			else
				startBtn.textContent = _('重新认证');

			if (truthy(state.config_stale)) {
				elStale.textContent = _('后端配置文件 /etc/autheap-client.conf 比 UCI 旧，可到“配置”页重新生成。');
				elStale.style.display = '';
			}
			else {
				elStale.style.display = 'none';
			}
		}

		function refresh() {
			return callStatus().then(function (s) {
				applyStatus(s);
				elMeta.textContent = _('最后更新 ') + hhmmss();
				return s;
			});
		}

		function control(action, label) {
			showError('');
			return callControl(action).then(function (r) {
				r = r || {};
				if (!truthy(r.res)) {
					showError(_('操作未成功：') + (r.output || _('（后端无输出）')));
				}
				else {
					ui.addNotification(null, E('p', {}, label + _('：操作成功')), 'info');
				}
				return refresh();
			}).catch(function (e) {
				showError(_('操作失败：') + err(e));
			});
		}

		function ctrlButton(label, action, cls) {
			var btn = E('button', {
				'class': 'cbi-button ' + (cls || 'cbi-button-action'),
				'click': function () { control(action, label); }
			}, label);
			return btn;
		}

		var startBtn = ctrlButton(_('启动'), 'start', 'cbi-button-apply');

		// ---- 自动刷新：自排队 setTimeout，避免请求堆积 ----
		var timer = null;
		var gen = 0;

		function schedule() {
			if (timer) { window.clearTimeout(timer); timer = null; }
			if (!autoChk.checked || document.hidden)
				return;
			timer = window.setTimeout(tick, REFRESH_MS);
		}

		function tick() {
			var my = gen;
			refresh().catch(function (e) {
				if (my === gen)
					showError(_('状态刷新失败：') + err(e));
			}).then(function () {
				if (my === gen)
					schedule();
			});
		}

		autoChk.addEventListener('change', function () {
			if (autoChk.checked) { gen++; tick(); }
			else schedule();
		});

		document.addEventListener('visibilitychange', function () {
			if (!document.hidden && autoChk.checked) {
				if (timer) { window.clearTimeout(timer); timer = null; }
				gen++;
				tick();
			}
		});

		window.addEventListener('beforeunload', function () {
			gen++;
			if (timer) window.clearTimeout(timer);
		});

		bootBtn.addEventListener('click', function () {
			control(bootBtn.getAttribute('data-boot') === '1' ? 'disable' : 'enable',
				bootBtn.textContent);
		});

		// ---- 网络测试 ----
		var elNetHost = E('input', {
			'type': 'text',
			'class': 'cbi-input-text',
			'placeholder': _('目标 IP / 主机[:端口]，可留空（只测本机与网关）'),
			'style': 'width:100%;max-width:420px'
		});
		var elNetMsg = E('span', { 'class': 'label' }, _('尚未测试'));
		var elNetOut = E('pre', { 'class': 'autheap-log' }, _('（尚未测试）'));

		if (state.beacon && state.beacon.dst)
			elNetHost.value = state.beacon.dst;

		function runNetTest() {
			elNetMsg.textContent = _('测试中…');
			elNetMsg.className = 'label warning';
			elNetOut.textContent = _('测试中…');

			callNetTest(String(elNetHost.value || '').trim(), '').then(function (r) {
				r = r || {};
				if (r.output) {
					elNetOut.textContent = r.output;
					elNetMsg.textContent = truthy(r.ok) ? _('完成：未发现明显问题') : _('完成：发现异常');
					elNetMsg.className = 'label ' + (truthy(r.ok) ? 'success' : 'important');
				}
				else {
					elNetMsg.textContent = _('完成：后端没有返回内容');
					elNetMsg.className = 'label important';
					elNetOut.textContent = _('后端没有返回 output 字段。原始返回：') + '\n' +
						JSON.stringify(r) + '\n\n' +
						_('可在设备上执行：') + '\n  /usr/bin/autheap --nettest';
				}
			}, function (e) {
				elNetMsg.textContent = _('测试失败');
				elNetMsg.className = 'label important';
				elNetOut.textContent = err(e);
			});
		}

		applyStatus(state);
		gen++;
		refresh().then(function () { schedule(); }).catch(function (e) {
			showError(_('无法读取状态：') + err(e));
			schedule();
		});

		var statusSection = E('div', { 'class': 'cbi-section' }, [
			E('h3', {}, _('运行状态')),
			E('div', { 'class': 'cbi-section-node' }, [
				row(_('认证状态'), elAuth),
				row(_('开机自启'), elBoot, _('由 /etc/rc.d 软链控制，可在下方切换。')),
				row(_('开机延迟'), elBootDelay, _('仅开机自启时等待；手动“启动”会立即认证。')),
				row(_('运行中'), elRun),
				row(_('认证网卡'), elIfname),
				row(_('用户名'), elUser),
				row(_('密码长度'), elPwd),
				row(_('插件'), elPlug),
				row(_('Beacon 目标'), elBeacon),
				row(_('版本'), elVer),
				row(_('核心'), elCore),
				elStale,
				E('div', { 'class': 'cbi-page-actions' }, [
					startBtn,
					ctrlButton(_('停止'), 'stop', 'cbi-button-reset'),
					ctrlButton(_('重启'), 'restart'),
					bootBtn
				]),
				E('div', { 'class': 'autheap-inline' }, [
					E('label', {}, [ autoChk, ' ' + _('自动刷新') ]),
					E('button', {
						'class': 'cbi-button cbi-button-action',
						'click': function () { gen++; tick(); }
					}, _('立即刷新')),
					elMeta
				]),
				elErr
			])
		]);

		var netSection = E('div', { 'class': 'cbi-section' }, [
			E('h3', {}, _('网络测试')),
			E('div', { 'class': 'cbi-section-descr' },
				_('检查本机接口地址 / 默认网关 / DNS，以及对目标的域名解析、TCP 连接（默认端口 80，' +
				  '也可写成 目标:端口）与 ICMP 连通性。目标留空则只检查本机与默认网关。')),
			E('div', { 'class': 'cbi-section-node' }, [
				row(_('测试目标'), E('div', { 'class': 'autheap-inline' }, [
					elNetHost,
					E('button', {
						'class': 'cbi-button cbi-button-action',
						'click': function () { runNetTest(); }
					}, _('开始测试')),
					elNetMsg
				])),
				row(_('测试结果'), elNetOut)
			])
		]);

		return E('div', { 'class': 'cbi-map' }, [
			viewStyle(),
			E('h2', {}, _('AuthEAP-Client')),
			E('div', { 'class': 'cbi-map-descr' },
				_('802.1x 认证状态与服务控制。状态每 2 秒自动刷新；修改设置请到“配置”页。')),
			statusSection,
			netSection
		]);
	}
});
