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

var callStatus = rpc.declare({
	object: 'luci.autheap_client',
	method: 'status',
	expect: {}
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

// ---- 通用控件 ----
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
		if (!spec) return;
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
	var inp = E('input', {
		'id': 'me_ifname', 'type': 'text', 'data-key': 'ifname',
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

	inp.addEventListener('focus', function () { if (shown) hidePop(); });
	document.addEventListener('click', function () { if (shown) hidePop(); });

	return E('span', { 'style': 'position:relative;display:inline-block;width:100%' }, [ inp, arrow, pop ]);
}

// ---- 插件多选下拉（选项由 list_plugins 动态提供）----
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

// 读取配置面板当前的输入值（含插件多选）-> 提交对象
function readForm(self) {
	var d = collect(self._inputs || []);
	applyPlugins(d, self._pluginHolder ? self._pluginHolder.get() : []);
	return d;
}

return L.view.extend({
	load: function () {
		// 状态 + 配置 + 网卡 + 插件，一次取齐
		return Promise.all([ callStatus(), callGet(), callListIfaces(), callListPlugins() ])
			.then(function (r) {
				return {
					st: r[0],
					cfg: r[1],
					ifaces: (r[2] && r[2].interfaces) || [],
					plugins: (r[3] && r[3].plugins) || []
				};
			});
	},

	// 状态面板刷新（不整页刷新，避免丢失未保存输入）
	updateStatus: function () {
		var self = this;
		return callStatus().then(function (st) {
			self._boot = (st.enabled === true || st.enabled === 1);
			var els = self._statusEls || {};
			if (els.boot)  els.boot.textContent  = self._boot ? '是' : '否';
			if (els.run)   els.run.textContent   = (st.running === true || st.running === 1) ? '是' : '否';
			if (els.ifname) els.ifname.textContent = st.ifname || '-';
			if (els.user)  els.user.textContent  = st.username || '-';
			if (els.plug)  els.plug.textContent  = fmtPlugins(st.plugins);
			if (els.log)   els.log.textContent   = esc(st.log || '(无日志)');
			if (els.bootBtn)
				els.bootBtn.textContent = self._boot ? '关闭开机自启' : '开启开机自启';
		});
	},

	render: function (data) {
		var self = this;
		var st = data.st || {};
		var cfg = data.cfg || {};
		var ifaces = data.ifaces || [];
		var inputs = [];
		var pluginHolder = { get: function () { return []; } };
		self._inputs = inputs;
		self._pluginHolder = pluginHolder;

		var boot = (st.enabled === true || st.enabled === 1);
		self._boot = boot;
		var running = (st.running === true || st.running === 1);

		// ---------- 状态面板 ----------
		var elBoot = E('strong', {}, boot ? '是' : '否');
		var elRun  = E('strong', {}, running ? '是' : '否');
		var elIfname = E('strong', {}, st.ifname || '-');
		var elUser   = E('strong', {}, st.username || '-');
		var elPlug   = E('strong', {}, fmtPlugins(st.plugins));
		var elLog = E('pre', { 'style': 'max-height:220px;overflow:auto;background:#222;color:#9f9;padding:8px' },
			esc(st.log || '(无日志)'));

		self._statusEls = {
			boot: elBoot, run: elRun, ifname: elIfname,
			user: elUser, plug: elPlug, log: elLog
		};

		function ctrlButton(label, action) {
			return E('button', {
				'class': 'cbi-button cbi-button-action',
				click: function () {
					callControl({ action: action }).then(function () {
						return self.updateStatus();
					}).catch(function (e) {
						alert('控制失败: ' + e.message);
					});
				}
			}, label);
		}
		var bootBtn = E('button', {
			'class': 'cbi-button cbi-button-action',
			click: function () {
				callControl({ action: self._boot ? 'disable' : 'enable' }).then(function () {
					return self.updateStatus();
				}).catch(function (e) { alert('控制失败: ' + e.message); });
			}
		}, boot ? '关闭开机自启' : '开启开机自启');
		self._statusEls.bootBtn = bootBtn;

		var statusPanel = E('div', { 'class': 'cbi-section', 'id': 'aec-status' }, [
			E('h3', {}, '状态'),
			row('开机自启', elBoot),
			row('运行中', elRun),
			row('接口', elIfname),
			row('用户名', elUser),
			row('插件', elPlug),
			E('div', { 'class': 'cbi-page-actions' }, [
				ctrlButton('启动', 'start'),
				ctrlButton('停止', 'stop'),
				ctrlButton('重启', 'restart'),
				bootBtn
			]),
			E('h4', {}, '最近日志'),
			elLog
		]);

		// ---------- 配置面板 ----------
		// 可用插件：由后端 list_plugins 动态提供
		var pluginOptions = (data.plugins || []).map(function (p) {
			return [ p.name, p.name, p.description || '' ];
		});
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
		var pluginBar = buildMultiSelect(pluginOptions, selNames, pluginHolder);

		var datalist = E('datalist', { 'id': 'aec_ifaces' }, ifaces.map(function (n) {
			return E('option', { value: n }, n);
		}));

		var configBody = E('div', {});
		var pluginDone = false;
		Object.keys(FIELDS).forEach(function (key) {
			var spec = FIELDS[key];

			// “插件”放在 heartbeat 之前
			if (key === 'heartbeat' && !pluginDone) {
				configBody.appendChild(E('div', { 'class': 'cbi-value' }, [
					E('div', { 'class': 'cbi-value-title' }, '启用插件（可多选，选项来自客户端已编译插件）'),
					E('div', { 'class': 'cbi-value-field' }, [ pluginBar ])
				]));
				pluginDone = true;
			}

			var el;
			if (spec.iface)
				el = buildIface(cfg[key], ifaces, inputs);
			else {
				el = inputFor(key, spec, cfg[key]);
				inputs.push(el);
			}
			configBody.appendChild(E('div', { 'class': 'cbi-value' }, [
				E('div', { 'class': 'cbi-value-title' }, spec.title),
				E('div', { 'class': 'cbi-value-field' }, [ el ])
			]));
		});
		if (!pluginDone) {
			configBody.appendChild(E('div', { 'class': 'cbi-value' }, [
				E('div', { 'class': 'cbi-value-title' }, '启用插件（可多选，选项来自客户端已编译插件）'),
				E('div', { 'class': 'cbi-value-field' }, [ pluginBar ])
			]));
		}
		configBody.appendChild(datalist);

		var configPanel = E('div', { 'class': 'cbi-section', 'id': 'aec-config' }, [
			E('h3', {}, '配置'),
			configBody
		]);

		return E('div', [
			E('h2', {}, 'AuthEAP-Client'),
			statusPanel,
			configPanel
		]);
	},

	// 官方页脚按钮：保存（仅写入 UCI/后端）
	handleSave: function (ev) {
		var d = readForm(this);
		return callSet(d).catch(function (e) {
			alert('保存失败: ' + e.message);
			throw e;
		});
	},

	// 官方页脚按钮：保存并应用（写入 + 启停）
	handleSaveApply: function (ev, mode) {
		var d = readForm(this);
		var self = this;
		return callSet(d).then(function () {
			return callControl({ action: d.enabled ? 'enable' : 'disable' });
		}).then(function () {
			return callControl({ action: d.enabled ? 'restart' : 'stop' });
		}).then(function () {
			window.setTimeout(function () { location.reload(); }, 800);
		}).catch(function (e) {
			alert('保存并应用失败: ' + e.message);
			throw e;
		});
	},

	// 官方页脚按钮：重置（放弃未保存改动）
	handleReset: function (ev) {
		location.reload();
	}
});
