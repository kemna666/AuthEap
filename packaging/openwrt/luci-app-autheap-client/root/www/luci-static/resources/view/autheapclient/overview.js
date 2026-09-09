'use strict';
'require rpc';

var callStatus = rpc.declare({
	object: 'luci.autheap_client',
	method: 'status',
	expect: {}
});

var callControl = rpc.declare({
	object: 'luci.autheap_client',
	method: 'control',
	expect: { res: true }
});

function row(label, value) {
	return E('div', { 'class': 'cbi-value' }, [
		E('div', { 'class': 'cbi-value-title' }, label),
		E('div', { 'class': 'cbi-value-field' }, [ E('strong', {}, value) ])
	]);
}

function esc(s) {
	return String(s)
		.replace(/&/g, '&amp;')
		.replace(/</g, '&lt;')
		.replace(/>/g, '&gt;')
		.replace(/"/g, '&quot;');
}

// 取日志末尾若干行并倒序（最新在上，方便查看）
function tailLog(s) {
	var lines = String(s || '').split('\n').filter(function (l) { return l.trim(); });
	return lines.slice(-50).reverse().join('\n');
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

function actionBtn(label, action) {
	return E('button', {
		'class': 'cbi-button cbi-button-action',
		click: function () {
			callControl({ action: action }).then(function () {
				window.setTimeout(function () { location.reload(); }, 1000);
			}).catch(function (e) {
				alert('操作失败: ' + e.message);
			});
		}
	}, label);
}

return L.view.extend({
	load: function () {
		return callStatus();
	},

	render: function (st) {
		var boot = (st.enabled === true || st.enabled === 1);
		var running = (st.running === true || st.running === 1);

		// 顶部“认证状态”横幅
		var authText, authColor;
		if (!running) {
			authText = '未运行';
			authColor = '#888888';
		}
		else if (st.auth === 'success') {
			authText = '认证成功';
			authColor = '#2e7d32';
		}
		else if (st.auth === 'failed') {
			authText = '认证失败';
			authColor = '#c62828';
		}
		else {
			authText = '认证中…';
			authColor = '#ef8f00';
		}
		var authBanner = E('div', {
			'style': 'padding:10px 14px;margin-bottom:10px;border:1px solid ' + authColor + ';' +
			         'border-radius:4px;background:rgba(127,127,127,.12);' +
			         'color:' + authColor + ';font-size:20px;font-weight:bold;text-align:center'
		}, authText);

		var bootBtn = E('button', {
			'class': 'cbi-button cbi-button-action',
			click: function () {
				callControl({ action: boot ? 'disable' : 'enable' }).then(function () {
					window.setTimeout(function () { location.reload(); }, 1000);
				}).catch(function (e) {
					alert('操作失败: ' + e.message);
				});
			}
		}, boot ? '关闭开机自启' : '开启开机自启');

		var section = E('div', { 'class': 'cbi-section' }, [
			authBanner,
			row('开机自启', boot ? '是' : '否'),
			row('运行中', running ? '是' : '否'),
			row('接口', st.ifname || '-'),
			row('用户名', st.username || '-'),
			row('插件', fmtPlugins(st.plugins)),
			E('div', { 'class': 'cbi-page-actions' }, [
				actionBtn('启动', 'start'),
				actionBtn('停止', 'stop'),
				actionBtn('重启', 'restart'),
				bootBtn
			]),
			E('h3', {}, '最近日志（最新在上）'),
			E('pre', { 'style': 'max-height:320px;overflow:auto;background:#222;color:#9f9;padding:8px' },
				esc(tailLog(st.log)) || '(无日志)')
		]);

		return E('div', { 'class': 'cbi-map' }, [
			E('h2', {}, 'AuthEAP-Client'),
			E('div', { 'class': 'cbi-map-descr' },
				'认证客户端运行状态、服务控制与最近日志。'),
			section
		]);
	}
});
