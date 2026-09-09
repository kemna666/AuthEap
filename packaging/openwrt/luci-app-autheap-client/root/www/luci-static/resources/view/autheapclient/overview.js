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
			E('h3', {}, '最近日志'),
			E('pre', { 'style': 'max-height:300px;overflow:auto;background:#222;color:#9f9;padding:8px' },
				esc(st.log || '(无日志)'))
		]);

		return E('div', { 'class': 'cbi-map' }, [
			E('h2', {}, 'AuthEAP-Client'),
			E('div', { 'class': 'cbi-map-descr' },
				'认证客户端运行状态、服务控制与最近日志。'),
			section
		]);
	}
});
