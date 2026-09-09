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

function button(name, label, action) {
	return E('button', {
		'class': 'cbi-button cbi-button-apply',
		click: function () {
			callControl({ action: action }).then(function () {
				window.setTimeout(function () { location.reload(); }, 1200);
			});
		}
	}, label);
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
		String(p.extra).split(/[\s,]+/).forEach(function (n) {
			if (n) parts.push(n);
		});
	}
	return parts.length ? parts.join(', ') : '（无）';
}

return L.view.extend({
	load: function () {
		return callStatus();
	},

	render: function (st) {
		var running = (st.running === true || st.running === 1);
		var boot = (st.enabled === true || st.enabled === 1);
		return E('div', [
			E('h2', {}, 'AuthEAP-Client 状态'),

			row('开机自启', boot ? '是' : '否'),
			row('运行中', running ? '是' : '否'),
			row('接口', st.ifname || '-'),
			row('用户名', st.username || '-'),
			row('插件', fmtPlugins(st.plugins)),

			E('div', { 'class': 'cbi-page-actions' }, [
				button('btn-start', '启动', 'start'),
				button('btn-stop', '停止', 'stop'),
				button('btn-restart', '重启', 'restart'),
				button('btn-boot', boot ? '关闭开机自启' : '开启开机自启', boot ? 'disable' : 'enable'),
				E('button', {
					'class': 'cbi-button cbi-button-action',
					click: function () {
						location.href = location.pathname.replace(/\/admin\/network\/autheapclient\/?(?:overview|config)?$/, '/admin/network/autheapclient/config');
					}
				}, '配置…'),
				E('button', {
					'class': 'cbi-button cbi-button-reset',
					click: function () { location.reload(); }
				}, '刷新')
			]),

			E('h3', {}, '最近日志'),
			E('pre', { 'style': 'max-height:300px;overflow:auto;background:#222;color:#9f9;padding:8px' },
				esc(st.log || '(无日志)'))
		]);
	}
});
