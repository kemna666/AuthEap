'use strict';
'require rpc';

var callGet = rpc.declare({
	object: 'luci.autheap_client',
	method: 'get_config',
	expect: {}
});

var callSet = rpc.declare({
	object: 'luci.autheap_client',
	method: 'set_config',
	expect: { res: true }
});

var callControl = rpc.declare({
	object: 'luci.autheap_client',
	method: 'control',
	expect: { res: true }
});

var callListIfaces = rpc.declare({
	object: 'luci.autheap_client',
	method: 'list_interfaces',
	expect: {}
});

var callListPlugins = rpc.declare({
	object: 'luci.autheap_client',
	method: 'list_plugins',
	expect: {}
});

// 表单字段：key -> { title, type: checkbox|text|select|number|password, options?, iface? }
var FIELDS = {
	enabled:          { title: '开机自启（系统启动时自动运行）', type: 'checkbox' },
	ifname:           { title: '认证网卡（可从下拉选择或手动输入）', type: 'text', iface: true },
	username:         { title: '用户名',   type: 'text' },
	password:         { title: '密码（存于 UCI，只读回显为空）', type: 'password' },
	backend:          { title: '收发包后端', type: 'select', options: { raw: 'raw (AF_PACKET)', pcap: 'pcap (需 feature)' } },
	heartbeat:        { title: '心跳间隔（秒，0 关闭）', type: 'number' },
	eap_bcast_addr:   { title: 'EAPOL-Start 广播地址 (0/1)', type: 'select', options: { 0: '0=标准', 1: '1=锐捷' } },
	dhcp_type:        { title: 'DHCP 方式', type: 'select', options: { 0: '0=不使用', 1: '1=二次认证', 2: '2=认证后', 3: '3=认证前' } },
	dhcp_script:      { title: 'DHCP 脚本', type: 'text' },
	service:          { title: '服务名', type: 'text' },
	version_str:      { title: '版本串', type: 'text' },
	fake_dns1:        { title: '主 DNS（可选）', type: 'text' },
	fake_dns2:        { title: '次 DNS（可选）', type: 'text' },
	fake_serial:      { title: '硬盘序列号（可选）', type: 'text' },
	max_dhcp_count:   { title: '二次认证等待 DHCP 次数上限', type: 'number' },
	max_retries:      { title: '阶段最大重试次数', type: 'number' },
	max_failures:     { title: '最大连续失败次数', type: 'number' },
	stage_timeout:    { title: '阶段超时（秒）', type: 'number' }
};

function inputFor(key, spec, value) {
	var id = 'me_' + key;
	var input;
	if (spec.type === 'checkbox') {
		input = E('input', { 'id': id, 'type': 'checkbox', 'data-key': key });
		input.checked = !!value;
	}
	else if (spec.type === 'select') {
		input = E('select', { 'id': id, 'data-key': key }, Object.keys(spec.options).map(function (k) {
			return E('option', { value: k }, spec.options[k]);
		}));
		input.value = String(value);
	}
	else if (spec.type === 'number') {
		input = E('input', { 'id': id, 'type': 'number', 'data-key': key });
		input.value = value;
	}
	else {
		input = E('input', { 'id': id, 'type': spec.type || 'text', 'data-key': key });
		if (spec.type !== 'password')
			input.value = value || '';
	}
	// 网卡：加 datalist（可下拉选择，也可手动输入）
	if (spec.iface) {
		input.setAttribute('list', 'aec_ifaces');
		input.setAttribute('placeholder', '例如 eth0 / enp5s0');
	}
	return input;
}

function collect(inputs) {
	var data = {};
	inputs.forEach(function (inp) {
		var key = inp.getAttribute('data-key');
		var spec = FIELDS[key];
		if (spec.type === 'checkbox') {
			data[key] = inp.checked;
		} else if (spec.type === 'number') {
			data[key] = parseInt(inp.value, 10) || 0;
		} else {
			data[key] = inp.value;
		}
	});
	return data;
}

// 认证网卡下拉：箭头弹出可选接口列表，同时保留手动输入能力
function buildIface(value, ifaces, inputs) {
	var id = 'me_ifname';
	var inp = E('input', {
		'id': id, 'type': 'text', 'data-key': 'ifname',
		'list': 'aec_ifaces', 'autocomplete': 'off',
		'placeholder': '例如 eth0 / enp5s0',
		'style': 'width:100%;box-sizing:border-box;padding-right:28px'
	});
	inp.value = value || '';
	inputs.push(inp);

	var pop = E('div', {
		'style': 'position:absolute;left:0;right:0;top:100%;margin-top:2px;display:none;' +
		         'max-height:200px;overflow:auto;background:#fff;color:#222;' +
		         'border:1px solid #999;box-shadow:0 2px 6px rgba(0,0,0,.35);' +
		         'z-index:100;font-size:13px;text-align:left'
	});

	var shown = false;
	function hidePop() { pop.style.display = 'none'; shown = false; }

	function showPop() {
		pop.innerHTML = '';
		var names = ifaces.slice();
		var cur = inp.value;
		if (cur && names.indexOf(cur) < 0)
			names.unshift(cur);

		if (!names.length) {
			pop.appendChild(E('div', { 'style': 'padding:5px 9px;color:#888' }, '(无可用接口，可手动输入)'));
		}
		else {
			names.forEach(function (n) {
				var it = E('div', {
					'style': 'padding:5px 9px;cursor:pointer;white-space:nowrap' +
					         (n === cur ? ';font-weight:bold' : '')
				}, n === cur ? [ n, ' ✓' ] : n);
				it.addEventListener('mouseenter', function () { it.style.background = '#e3f2fd'; });
				it.addEventListener('mouseleave', function () { it.style.background = ''; });
				it.addEventListener('click', function (ev) {
					ev.stopPropagation();
					inp.value = n;
					hidePop();
				});
				pop.appendChild(it);
			});
		}

		pop.appendChild(E('div', { 'style': 'padding:5px 9px;color:#888;cursor:pointer;border-top:1px dashed #ccc' },
			'✎ 手动输入…'));

		pop.lastChild.addEventListener('click', function (ev) {
			ev.stopPropagation();
			hidePop();
			inp.focus();
		});

		pop.style.display = 'block';
		shown = true;
	}

	var arrow = E('button', {
		'type': 'button', 'tabindex': '-1',
		'class': 'cbi-button cbi-button-neutral',
		'style': 'position:absolute;right:0;top:1px;bottom:1px;border:0;background:transparent;' +
		         'cursor:pointer;padding:0 9px;color:#555;font-size:13px;line-height:1;box-shadow:none'
	}, '▾');
	arrow.addEventListener('click', function (ev) {
		ev.preventDefault();
		ev.stopPropagation();
		if (shown) hidePop(); else showPop();
	});

	// 点击其它位置时收起
	inp.addEventListener('focus', function () { if (shown) hidePop(); });
	document.addEventListener('click', function () { if (shown) hidePop(); });

	return E('span', { 'style': 'position:relative;display:inline-block;width:100%' }, [ inp, arrow, pop ]);
}

// ---- 插件多选下拉 ----
// options: [ [value, label, description?], ... ]（由 list_plugins 动态提供）
function buildMultiSelect(options, selected, holder) {
	var cur = (selected || []).slice();
	var shown = false;

	function labelOf(v) {
		for (var i = 0; i < options.length; i++)
			if (options[i][0] === v)
				return options[i][1];
		return v;
	}

	var text = E('span', { 'style': 'display:block;overflow:hidden;text-overflow:ellipsis;white-space:nowrap' },
		cur.length ? cur.map(labelOf).join('、') : '（未选择）');

	var bar = E('div', {
		'style': 'position:relative;width:100%;box-sizing:border-box;border:1px solid #c9c9c9;' +
		         'border-radius:2px;background:#fff;cursor:pointer;padding:5px 30px 5px 8px;' +
		         'font-size:14px;color:#333;min-height:32px;line-height:1.4'
	}, [ text ]);

	var arrow = E('span', { 'style': 'position:absolute;right:10px;top:50%;transform:translateY(-50%);color:#666' }, '▾');
	bar.appendChild(arrow);

	var pop = E('div', {
		'style': 'position:absolute;left:0;right:0;top:100%;margin-top:2px;display:none;' +
		         'max-height:220px;overflow:auto;background:#fff;color:#222;' +
		         'border:1px solid #999;box-shadow:0 2px 6px rgba(0,0,0,.35);' +
		         'z-index:100;font-size:13px;text-align:left'
	});
	bar.appendChild(pop);

	function refreshText() {
		text.textContent = cur.length ? cur.map(labelOf).join('、') : '（未选择）';
	}

	function hidePop() { pop.style.display = 'none'; shown = false; }

	function showPop() {
		pop.innerHTML = '';
		options.forEach(function (opt) {
			var v = opt[0];
			var row = E('div', { 'style': 'padding:5px 8px;cursor:pointer;white-space:nowrap' });
			var mark = E('span', { 'style': 'display:inline-block;width:20px;color:#2e7d32' }, '☐');
			var lbl = E('span', {}, opt[1]);
			row.appendChild(mark);
			row.appendChild(lbl);
			if (opt[2])
				row.appendChild(E('span', { 'style': 'color:#888;font-size:12px' }, ' — ' + opt[2]));
			row.addEventListener('click', function (ev) {
				ev.stopPropagation();
				var i = cur.indexOf(v);
				if (i >= 0) cur.splice(i, 1); else cur.push(v);
				mark.textContent = cur.indexOf(v) >= 0 ? '☑' : '☐';
				refreshText();
			});
			pop.appendChild(row);
		});
		pop.style.display = 'block';
		shown = true;
	}

	bar.addEventListener('click', function (ev) {
		ev.stopPropagation();
		if (shown) hidePop(); else showPop();
	});
	document.addEventListener('click', function () { if (shown) hidePop(); });

	if (holder) holder.get = function () { return cur.slice(); };
	return bar;
}

// 把多选结果映射回后端字段：rjv3/printer/stats 用布尔，其它名字进 extra_plugins
function applyPlugins(d, sel) {
	d.plugin_rjv3 = sel.indexOf('rjv3') >= 0;
	d.plugin_printer = sel.indexOf('printer') >= 0;
	d.plugin_stats = sel.indexOf('stats') >= 0;
	d.extra_plugins = sel.filter(function (n) {
		return n !== 'rjv3' && n !== 'printer' && n !== 'stats';
	}).join(' ');
}

return L.view.extend({
	load: function () {
		// 取配置、网卡列表、可用插件列表
		return Promise.all([ callGet(), callListIfaces(), callListPlugins() ]).then(function (r) {
			return { cfg: r[0], ifaces: (r[1] && r[1].interfaces) || [], plugins: (r[2] && r[2].plugins) || [] };
		});
	},

	render: function (data) {
		var cfg = data.cfg;
		var ifaces = data.ifaces || [];
		var inputs = [];
		var datalist = E('datalist', { 'id': 'aec_ifaces' }, ifaces.map(function (n) {
			return E('option', { value: n }, n);
		}));

		var sections = E('div', {});

		// 可用插件：由后端 list_plugins 动态读取（读取客户端 --list-plugins）
		var pluginOptions = (data.plugins || []).map(function (p) {
			return [ p.name, p.name, p.description || '' ];
		});
		// 当前已启用：旧布尔字段 + extra_plugins 里的名字（不在列表中的也补成选项以免丢失）
		var selNames = [];
		[ 'rjv3', 'printer', 'stats' ].forEach(function (k) {
			if (cfg['plugin_' + k]) selNames.push(k);
		});
		if (cfg.extra_plugins)
			String(cfg.extra_plugins).split(/[\s,]+/).forEach(function (n) {
				if (n) selNames.push(n);
			});
		selNames.forEach(function (n) {
			var exists = pluginOptions.some(function (o) { return o[0] === n; });
			if (!exists) pluginOptions.push([ n, n, '' ]);
		});

		var pluginHolder = { get: function () { return []; } };
		var pluginBar = buildMultiSelect(pluginOptions, selNames, pluginHolder);
		var pluginField = E('div', { 'class': 'cbi-value' }, [
			E('div', { 'class': 'cbi-value-title' }, '启用插件（可多选，选项来自客户端已编译插件）'),
			E('div', { 'class': 'cbi-value-field' }, [ pluginBar ])
		]);

		var pluginDone = false;
		Object.keys(FIELDS).forEach(function (key) {
			var spec = FIELDS[key];

			// 在 heartbeat 之前插入“插件”多选
			if (key === 'heartbeat' && !pluginDone) {
				sections.appendChild(pluginField);
				pluginDone = true;
			}

			var el;
			if (spec.iface)
				el = buildIface(cfg[key], ifaces, inputs);
			else {
				el = inputFor(key, spec, cfg[key]);
				inputs.push(el);
			}
			sections.appendChild(E('div', { 'class': 'cbi-value' }, [
				E('div', { 'class': 'cbi-value-title' }, spec.title),
				E('div', { 'class': 'cbi-value-field' }, [ el ])
			]));
		});
		if (!pluginDone)
			sections.appendChild(pluginField);
		sections.appendChild(datalist);

		var applyBtn = E('button', { 'class': 'cbi-button cbi-button-apply' }, '保存并应用');
		applyBtn.addEventListener('click', function () {
			var d = collect(inputs);
			applyPlugins(d, pluginHolder.get());
			callSet(d).then(function () {
				return callControl({ action: d.enabled ? 'enable' : 'disable' });
			}).then(function () {
				return callControl({ action: d.enabled ? 'restart' : 'stop' });
			}).then(function () {
				window.setTimeout(function () { location.href = location.href.replace(/\/config$/, '/overview'); }, 1500);
			});
		});

		var saveBtn = E('button', { 'class': 'cbi-button cbi-button-apply' }, '仅保存（不启停）');
		saveBtn.addEventListener('click', function () {
			var d = collect(inputs);
			applyPlugins(d, pluginHolder.get());
			callSet(d).then(function () {
				return callControl({ action: d.enabled ? 'enable' : 'disable' });
			});
		});

		return E('div', [
			E('h2', {}, 'AuthEAP-Client 配置'),
			sections,
			E('div', { 'class': 'cbi-page-actions' }, [
				applyBtn,
				saveBtn,
				E('button', { 'class': 'cbi-button cbi-button-reset', click: function () { location.reload(); } }, '撤销'),
				E('button', {
					'class': 'cbi-button cbi-button-action',
					click: function () {
						location.href = location.pathname.replace(/\/admin\/network\/autheapclient\/?(?:overview|config)?$/, '/admin/network/autheapclient');
					}
				}, '返回状态')
			])
		]);
	}
});
