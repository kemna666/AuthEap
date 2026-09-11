'use strict';
'require rpc';

var callStatus = rpc.declare({
	object: 'luci.autheap_client',
	method: 'status',
	expect: {}
});

// 本页只有两块内容：认证状态/服务控制 + 网络测试
// 注意：LuCI 的 `expect` 只取【第一个键】且类型不符就用默认值，
// 想拿整个返回对象就不要写 expect（后端 res/ok 返回的是 1/0 数字）。
// 同理，**声明了 params 才按位置映射参数**；不声明 params 时传入的参数会被全部丢弃
// （曾经因此把“停止”发成了空参数 → 后端落到默认 restart）。
var callControl = rpc.declare({
	object: 'luci.autheap_client',
	method: 'control',
	params: [ 'action' ]
});

// 网络测试（后端执行 autheap --nettest，返回纯文本报告）
var callNetTest = rpc.declare({
	object: 'luci.autheap_client',
	method: 'nettest',
	params: [ 'host', 'ifname' ]
});

var ACL_HINT = '（若提示 Access denied：请在设备上执行 /etc/init.d/rpcd restart 并重新登录 LuCI，再 Ctrl+F5 强刷）';

var REFRESH_MS = 1500;   // 状态拉取间隔

function row(label, el) {
	return E('div', { 'class': 'cbi-value' }, [
		E('div', { 'class': 'cbi-value-title' }, label),
		E('div', { 'class': 'cbi-value-field' }, [ el ])
	]);
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
	if (p.beacon) parts.push('beacon');
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
		var elBeacon = E('strong', {});
		var elVer = E('strong', {});
		var elCore = E('strong', {});
		var elMeta = E('em', {}, '尚未刷新');
		var elErr  = E('div', { 'style': 'color:#c62828;font-size:12px' });
		var bootBtn = E('button', { 'class': 'cbi-button cbi-button-action' });
		var autoChk = E('input', { 'type': 'checkbox', 'checked': true });
		var autoLbl = E('label', {}, [ autoChk, ' 自动刷新（状态，每 1.5 秒）' ]);

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
			elPwd.textContent = (typeof s.pwdlen === 'number' ? s.pwdlen : '-') + ' 位';
			elPlug.textContent = fmtPlugins(s.plugins);
			elBeacon.textContent = (s.beacon && s.beacon.dst)
				? (s.beacon.dst + ':' + (s.beacon.port || 8083) + '，每 ' + (s.beacon.interval || 0) + ' 秒')
				: '未启用';
			elVer.textContent = s.version || '-';
			elCore.textContent = s.core || '-';

			bootBtn.textContent = boot ? '关闭开机自启' : '开启开机自启';
			bootBtn.setAttribute('data-boot', boot ? '1' : '0');
		}

		function control(action) {
					return callControl(action)
				.then(function (r) {
					r = r || {};
					if (!r.res) {
						// 后端返回 res=0（1/0），带上 init.d/syslog 的原因
						elErr.style.color = '#c62828';
						elErr.textContent = '操作未成功：' + (r.output || '（后端无输出）') +
							'；常见原因：未填账号或密码（服务会拒绝启动）、二进制缺失。' + ACL_HINT;
					} else {
						elErr.style.color = '#2e7d32';
						elErr.textContent = r.output ? ('操作成功（' + action + '）：' + r.output)
							: ('操作成功（' + action + '）');
					}
					return callStatus();
				})
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

			callStatus().then(function (s) {
				if (my !== gen) return;
				applyStatus(s || {});
			}, function (e) {
				if (my === gen) errs.push('状态刷新失败: ' + err(e));
			}).then(function () {
				if (my !== gen) return;
				elMeta.textContent = '最后更新 ' + hhmmss();
				elErr.textContent = errs.join(' | ');
				schedule();
			});
		}

		autoChk.addEventListener('change', function () {
			if (autoChk.checked) { gen++; tick(); } else schedule();
		});

		window.addEventListener('beforeunload', function () {
			gen++;
			if (timer) window.clearTimeout(timer);
		});

		applyStatus(st || {});
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
			row('密码长度', elPwd),
			row('插件', elPlug),
			row('beacon 目标', elBeacon),
			row('版本', elVer),
			row('核心', elCore),
			E('div', { 'class': 'cbi-page-actions' }, [
				ctrlButton('启动', 'start'),
				ctrlButton('停止', 'stop'),
				ctrlButton('重启', 'restart'),
				bootBtn
			]),
			E('div', { 'style': 'display:flex;align-items:center;gap:12px;font-size:12px;margin-top:10px' }, [
				autoLbl,
				E('button', {
					'class': 'cbi-button cbi-button-neutral',
					click: function () { gen++; tick(); }
				}, '立即刷新'),
				elMeta
			]),
			elErr
		]);

		// ---- 网络测试 ----
		var elNetHost = E('input', {
			'type': 'text',
			'class': 'cbi-input-text',
			'placeholder': '目标 IP / 主机[:端口]，可留空（只测本机与网关）',
			'style': 'min-width:280px'
		});
		var elNetMsg = E('em', { 'style': 'font-size:12px' });
		var elNetOut = E('pre', {
			'style': 'margin:8px 0 0;padding:8px 10px;max-height:340px;overflow:auto;' +
			         'font-size:12px;line-height:1.45;white-space:pre-wrap;' +
			         'border:1px solid #888;border-radius:4px;background:rgba(127,127,127,.08)'
		}, '（尚未测试）');

		if (st && st.beacon && st.beacon.dst)
			elNetHost.value = st.beacon.dst;

		function runNetTest() {
			elNetMsg.style.color = '';
			elNetMsg.textContent = '测试中…';
			elNetOut.textContent = '测试中…';
			// 注意：callNetTest 声明了 params，必须按位置传参
			callNetTest(String(elNetHost.value || '').trim(), '')
				.then(function (r) {
					r = r || {};
					if (r.output) {
						elNetOut.textContent = r.output;
						elNetMsg.textContent = r.ok ? '完成：未发现明显问题'
							: '完成：发现异常（见结果中标 ✘ 的行）';
						elNetMsg.style.color = r.ok ? '#2e7d32' : '#c62828';
					} else {
						elNetMsg.textContent = '完成：后端没有返回内容';
						elNetMsg.style.color = '#c62828';
						elNetOut.textContent =
							'后端没有返回 output 字段。原始返回：\n' +
							JSON.stringify(r) + '\n\n' +
							'可在设备上执行下面命令定位：\n' +
							'  /usr/bin/autheap --nettest\n' +
							'  echo \'{"host":""}\' | /usr/libexec/rpcd/luci.autheap_client call nettest\n\n' +
							ACL_HINT;
					}
				}, function (e) {
					elNetMsg.textContent = '';
					elNetOut.textContent = '测试失败: ' + (e && (e.message || e)) + '\n' + ACL_HINT;
				});
		}

		var netSection = E('div', { 'class': 'cbi-section' }, [
			E('h3', {}, '网络测试'),
			E('div', { 'class': 'cbi-map-descr' },
				'检查本机接口地址 / 默认网关 / DNS，以及对目标的域名解析、TCP 连接（默认端口 80，' +
				'也可写成 目标:端口）与 ICMP 连通性。ICMP 需要 root 权限（服务本身以 root 运行）；' +
				'目标留空则只检查本机与默认网关。'),
			E('div', { 'style': 'display:flex;align-items:center;gap:8px;flex-wrap:wrap;margin:6px 0' }, [
				elNetHost,
				E('button', {
					'class': 'cbi-button cbi-button-action',
					click: function () { runNetTest(); }
				}, '开始测试'),
				elNetMsg
			]),
			elNetOut
		]);

		return E('div', { 'class': 'cbi-map' }, [
			E('h2', {}, 'AuthEAP-Client'),
			E('div', { 'class': 'cbi-map-descr' }, '认证状态与服务控制（每 1.5 秒自动刷新）。'),
			section,
			netSection
		]);
	}
});
