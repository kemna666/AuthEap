'use strict';

'require form';
'require uci';
'require view';
'require rpc';
'require ui';
'require tools.widgets as widgets';

// 说明：LuCI 的 rpc.declare 只有声明了 `params` 才会把位置参数映射成命名参数。
// `expect` 也不是字段白名单，只取第一个 key；这里需要完整返回对象，故省略。
var callListPlugins = rpc.declare({
	object: 'luci.autheap_client',
	method: 'list_plugins'
});

var callStatus = rpc.declare({
	object: 'luci.autheap_client',
	method: 'status'
});

var callControl = rpc.declare({
	object: 'luci.autheap_client',
	method: 'control',
	params: [ 'action' ]
});

var callGenConfig = rpc.declare({
	object: 'luci.autheap_client',
	method: 'gen_config'
});

var callConfigFile = rpc.declare({
	object: 'luci.autheap_client',
	method: 'config_file'
});

var DEFAULT_PLUGINS = [ 'rjv3', 'printer', 'stats', 'report', 'beacon' ];
var PLUGIN_FALLBACK_DESC = {
	rjv3:    '锐捷 v3/v4 私有认证算法（含二次认证与心跳）',
	printer: '打印流经插件的收发帧，便于调试',
	stats:   '统计认证期间收发报文数量与字节数',
	report:  '如实上报本机网卡/MAC/IP/网关/DNS/主机名',
	beacon:  '周期性向指定目标发送 TCP 保活数据包'
};

function truthy(v) {
	return v === true || v === 1 || v === '1' || v === 'true' || v === 'on';
}

function asList(v) {
	if (v == null || v === '')
		return [];
	if (Array.isArray(v))
		return v.map(String);

	return String(v).split(/[\s,]+/).filter(function (n) { return n; });
}

// 视图级提示条：使用 LuCI/Bootstrap 主题自带的 alert-message 样式，不写死配色。
function notice(kind, text) {
	return E('div', { 'class': 'alert-message ' + kind }, E('p', {}, text));
}

function viewStyle() {
	return E('style', {}, [
		'.autheap-log {',
		'  font-family: var(--font-monospace, monospace);',
		'  font-size: 12px;',
		'  line-height: 1.45;',
		'  white-space: pre-wrap;',
		'  word-break: break-all;',
		'  max-height: 360px;',
		'  overflow: auto;',
		'  padding: 8px 10px;',
		'  margin: 0;',
		'  background: var(--background-color-high, rgba(127,127,127,.08));',
		'  color: var(--text-color-high, inherit);',
		'  border: 1px solid var(--border-color-medium, rgba(127,127,127,.35));',
		'  border-radius: var(--border-radius, 4px);',
		'}',
		'.autheap-inline { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }'
	].join('\n'));
}

return view.extend({
	load: function () {
		var self = this;

		return Promise.all([
			uci.load('autheap-client'),
			callListPlugins().catch(function () { return { plugins: [] }; }),
			callStatus().catch(function () { return null; }),
			callConfigFile().catch(function () { return null; })
		]).then(function (r) {
			self._available = (r[1] && r[1].plugins) || [];
			self._status = r[2] || null;
			self._configFile = r[3] || null;
			// 旧配置迁移只在内存里做，不调用 uci.set：避免一打开配置页就产生
			// “未保存的修改”（那会让保存/应用按钮和状态看起来不同步）。
			if (uci.get('autheap-client', 'main', 'plugins') == null) {
				var legacy = [];
				[ [ 'plugin_rjv3', 'rjv3', true ], [ 'plugin_printer', 'printer', false ], [ 'plugin_stats', 'stats', false ] ]
					.forEach(function (kv) {
						var v = uci.get('autheap-client', 'main', kv[0]);
						// 与 gen-config/status 的默认值保持一致：旧配置里缺少
						// plugin_rjv3 时默认启用 rjv3，printer/stats 默认关闭。
						if (v == null ? kv[2] : (v == '1' || v == 'true' || v == 'on'))
							legacy.push(kv[1]);
					});
				asList(uci.get('autheap-client', 'main', 'extra_plugins')).forEach(function (n) {
					if (legacy.indexOf(n) < 0)
						legacy.push(n);
				});
				self._legacyPlugins = legacy;
			}
			else {
				self._legacyPlugins = null;
			}
		});
	},

	render: function () {
		var self = this;
		var avail = this._available || [];
		var pluginChoices = [];
		var seenPlugins = {};
		var names;

		// 后端 --list-plugins 的结果优先；再补齐内置插件，保证二进制不可用时
		// 插件菜单仍然可用。最终按“可用顺序 + 内置顺序”展示。
		avail.forEach(function (p) {
			if (!p || !p.name || seenPlugins[p.name])
				return;
			seenPlugins[p.name] = true;
			pluginChoices.push({
				name: p.name,
				description: p.description || PLUGIN_FALLBACK_DESC[p.name] || _('由 autheap 提供')
			});
		});
		DEFAULT_PLUGINS.forEach(function (name) {
			if (seenPlugins[name])
				return;
			seenPlugins[name] = true;
			pluginChoices.push({
				name: name,
				description: PLUGIN_FALLBACK_DESC[name] || _('由 autheap 提供')
			});
		});
		names = pluginChoices.map(function (p) { return p.name; }).join('、');

		var m = new form.Map('autheap-client', _('AuthEAP-Client'),
			_('802.1x / EAP-MD5 客户端设置。保存并应用后会自动重新生成后端配置，并按需重启服务。'));

		// 单一定名 section + 原生 tab 是 LuCI 的标准写法：
		// 所有选项都落在同一个 UCI section（main）里，不会出现多个 section
		// 共用同一个 DOM id / 保存时互相覆盖的问题。
		var s = m.section(form.NamedSection, 'main', 'autheap-client');
		s.hidetitle = true;

		s.tab('basic',   _('基本设置'), _('认证网卡、账号与运行方式'));
		s.tab('plugins', _('插件'),     _('按需启用认证插件；插件名由已安装的 autheap 提供'));
		s.tab('rjv3',    _('锐捷'),     _('锐捷 v3/v4 插件参数；仅当插件列表启用 rjv3 时生效'));
		s.tab('beacon',  _('Beacon'),   _('周期性向指定目标发送 TCP 保活数据包'));
		s.tab('log',     _('日志'),     _('后端日志文件与自动轮转清理'));
		s.tab('advanced', _('高级'),    _('重连、单实例与调试参数'));

		var o;

		// ================= Tab: 基本设置 =================
		o = s.taboption('basic', form.Flag, 'enabled', _('开机自启'));
		o.default = '0';
		o.description = _('保存并应用后同步到 /etc/rc.d 开机软链；不会启动或停止当前进程。');

		o = s.taboption('basic', form.Value, 'boot_delay', _('开机启动后延迟认证（秒）'));
		o.datatype = 'uinteger';
		o.default = '600';
		o.placeholder = '600';
		o.description = _('仅开机自启时生效，默认 600 秒；0 = 立即认证。状态页点“启动”会立即认证。');

		o = s.taboption('basic', form.Flag, 'mline', _('多线多拨（mline）'));
		o.default = '0';
		o.description = _('启用后忽略上面的单线网卡，改为按下方“多线多拨线路”逐条启动独立认证进程；每条线路可覆盖账号。');

		o = s.taboption('basic', widgets.DeviceSelect, 'ifname', _('认证网卡'));
		o.noaliases = true;
		o.rmempty = false;
		o.default = 'eth0';
		o.depends('mline', '0');
		o.description = _('单线模式使用；多线模式请到下方“多线多拨线路”逐条配置。');

		o = s.taboption('basic', form.Value, 'username', _('用户名'));
		o.rmempty = true;
		o.placeholder = _('学号 / 账号');
		o.description = _('单线模式必填；多线模式可留空，由各线路提供或继承。');

		o = s.taboption('basic', form.Value, 'password', _('密码'));
		o.password = true;
		o.rmempty = true;
		o.placeholder = _('请输入认证密码');
		o.description = _('单线模式必填；多线模式可留空由线路提供。密码保存在 /etc/config/autheap-client，请限制该文件权限。');

		o = s.taboption('basic', form.ListValue, 'backend', _('收发包后端'));
		o.value('raw', _('raw（Linux AF_PACKET，推荐）'));
		o.value('pcap', _('pcap（需要在编译时启用 backend-pcap）'));
		o.default = 'raw';

		o = s.taboption('basic', form.Flag, 'daemon', _('后台守护模式'));
		o.default = '0';
		o.description = _('一般交给 procd 托管即可；仅在没有 procd 或需要独立后台进程时开启。');

		o = s.taboption('basic', form.Value, 'max_retries', _('每阶段最大重试次数'));
		o.datatype = 'uinteger';
		o.default = '3';

		o = s.taboption('basic', form.Value, 'max_failures', _('最大连续失败次数'));
		o.datatype = 'uinteger';
		o.default = '3';

		o = s.taboption('basic', form.Value, 'stage_timeout', _('阶段超时（秒）'));
		o.datatype = 'uinteger';
		o.default = '5';

		// ================= Tab: 插件 =================
		// 原生多选下拉：列出后端实际编译进二进制的插件，并显示说明；
		// create=true 允许输入列表之外的自定义插件名（需已编译进客户端）。
		o = s.taboption('plugins', form.MultiValue, 'plugins', _('启用的插件'),
			_('从下拉列表中多选；也允许输入自定义插件名。可用：') + names);
		o.rmempty = true;
		o.create = true;
		o.placeholder = _('选择要启用的插件（可多选）');
		pluginChoices.forEach(function (p) {
			o.value(p.name, p.name, p.description);
		});
		self._pluginsOpt = o;
		// 旧配置没有 plugins 列表时，表单仍显示旧布尔开关推出的插件，
		// 但不在打开页面时写入 UCI（避免产生幽灵“未保存修改”）。
		if (self._legacyPlugins) {
			o.cfgvalue = function () {
				var v = uci.get('autheap-client', 'main', 'plugins');
				if (v == null)
					return self._legacyPlugins;
				return asList(v);
			};
		}

		o = s.taboption('plugins', form.Flag, 'printer_log_recv', _('打印收到的帧'),
			_('仅对 printer 插件有效；开启后收到的帧也会写入日志。'));
		o.default = '0';
		o.depends({ plugins: 'printer', '!contains': true });

		// ================= Tab: 锐捷 =================
		o = s.taboption('rjv3', form.Value, 'heartbeat', _('心跳间隔（秒）'));
		o.datatype = 'uinteger';
		o.default = '20';
		o.description = _('0 表示关闭心跳。');

		o = s.taboption('rjv3', form.ListValue, 'eap_bcast_addr', _('EAPOL-Start 广播地址'));
		o.value('0', _('0 = 标准'));
		o.value('1', _('1 = 锐捷'));
		o.default = '0';

		o = s.taboption('rjv3', form.ListValue, 'dhcp_type', _('DHCP 方式'));
		o.value('0', _('0 = 不使用'));
		o.value('1', _('1 = 二次认证'));
		o.value('2', _('2 = 认证后'));
		o.value('3', _('3 = 认证前'));
		o.default = '2';

		o = s.taboption('rjv3', form.Value, 'dhcp_script', _('DHCP 脚本'));
		o.placeholder = _('例如：dhclient eth0');
		o.rmempty = true;
		o.depends('dhcp_type', '1');
		o.depends('dhcp_type', '2');
		o.depends('dhcp_type', '3');

		o = s.taboption('rjv3', form.Value, 'service', _('服务名'));
		o.default = 'internet';
		o.rmempty = true;

		o = s.taboption('rjv3', form.Value, 'version_str', _('版本串'));
		o.default = 'RG-SU For Linux V1.0';
		o.rmempty = true;

		o = s.taboption('rjv3', form.Value, 'fake_dns1', _('主 DNS（可选）'));
		o.datatype = 'ip4addr';
		o.rmempty = true;

		o = s.taboption('rjv3', form.Value, 'fake_dns2', _('次 DNS（可选）'));
		o.datatype = 'ipaddr';
		o.rmempty = true;

		o = s.taboption('rjv3', form.Value, 'fake_serial', _('硬盘序列号（可选）'));
		o.rmempty = true;

		o = s.taboption('rjv3', form.Value, 'max_dhcp_count', _('二次认证等待 DHCP 次数上限'));
		o.datatype = 'uinteger';
		o.default = '3';
		o.depends('dhcp_type', '1');

		// ================= Tab: Beacon =================
		o = s.taboption('beacon', form.Value, 'beacon_dst', _('目标 IP / 主机名'));
		o.placeholder = _('例如 203.0.113.10');
		o.rmempty = true;
		o.description = _('填了目标且间隔不为 0 时自动启用 beacon 插件。');

		o = s.taboption('beacon', form.Value, 'beacon_port', _('目标端口'));
		o.datatype = 'port';
		o.default = '8083';

		o = s.taboption('beacon', form.Value, 'beacon_interval', _('发送间隔（秒）'));
		o.datatype = 'uinteger';
		o.default = '60';
		o.description = _('0 表示禁用。');

		o = s.taboption('beacon', form.Value, 'beacon_payload', _('载荷（十六进制）'));
		o.placeholder = _('留空 = 内置默认载荷；如需覆盖请粘贴抓包 Data（偶数长度十六进制）');
		o.rmempty = true;
		o.validate = function (section_id, value) {
			if (value == null || value === '')
				return true;
			if (!/^[0-9a-fA-F]+$/.test(value))
				return _('只能包含十六进制字符（0-9 / a-f）');
			if (value.length % 2 !== 0)
				return _('十六进制长度必须为偶数');
			return true;
		};

		// ================= Tab: 日志 =================
		o = s.taboption('log', form.Value, 'log_file', _('日志文件'));
		o.default = '/var/log/autheap-client.log';
		o.rmempty = true;

		o = s.taboption('log', form.Value, 'log_max_kb', _('单文件大小上限（KB）'));
		o.datatype = 'uinteger';
		o.default = '256';

		o = s.taboption('log', form.Value, 'log_keep', _('保留历史份数'));
		o.datatype = 'uinteger';
		o.default = '3';
		o.description = _('轮转后保留 .1/.2… 的份数，0 表示超限直接清空。');

		o = s.taboption('log', form.Value, 'log_rotate_secs', _('检查/清理间隔（秒）'));
		o.datatype = 'uinteger';
		o.default = '300';

		// ================= Tab: 高级 =================
		o = s.taboption('advanced', form.Flag, 'restart_on_logoff', _('掉线后自动重连'));
		o.default = '1';

		o = s.taboption('advanced', form.Value, 'wait_after_fail', _('失败后等待重试（秒）'));
		o.datatype = 'uinteger';
		o.default = '30';

		o = s.taboption('advanced', form.Value, 'state_file', _('认证状态文件'));
		o.default = '/var/run/autheap-client.auth';
		o.placeholder = '/var/run/autheap-client.auth';
		o.description = _('状态页读取该文件；单线默认 /var/run/autheap-client.auth，多线每条线路自动独立。');

		o = s.taboption('advanced', form.Value, 'pid_file', _('单实例锁文件'));
		o.default = '/var/run/autheap.pid';
		o.placeholder = '/var/run/autheap.pid';
		o.rmempty = true;
		o.description = _('默认 /var/run/autheap.pid；填 - 表示不限制单实例（后端识别 - 为空值）。');

		// ================= 多线多拨线路 =================
		// GridSection 是 LuCI 原生的“表格 + 弹窗编辑 + 增删/排序”控件，比把
		// 动态 section 硬塞进 NamedSection tab 更稳定；每行对应一个 UCI line section。
		var sl = m.section(form.GridSection, 'line', _('多线多拨线路'),
			_('每行一条独立认证线路。父接口非空时自动创建 macvlan（单线多拨）；留空则使用已有物理接口。账号/密码留空继承主配置。'));
		sl.anonymous = true;
		sl.addremove = true;
		sl.sortable = true;

		o = sl.option(form.Flag, 'enabled', _('启用'));
		o.default = '1';
		o.editable = true;

		o = sl.option(form.Value, 'ifname', _('接口'));
		o.rmempty = false;
		o.placeholder = _('mline0');
		o.editable = true;
		o.description = _('实际用于认证的接口，最多 15 个字符；macvlan 线路可填尚不存在的名称。');

		o = sl.option(widgets.DeviceSelect, 'parent', _('父接口'));
		o.noaliases = true;
		o.rmempty = true;
		o.placeholder = _('eth0');
		o.editable = true;
		o.description = _('非空时自动创建 macvlan；留空则使用已有接口。');

		o = sl.option(form.Value, 'mac', _('MAC 地址'));
		o.datatype = 'macaddr';
		o.rmempty = true;
		o.placeholder = _('留空自动生成');
		o.editable = true;
		o.description = _('仅 macvlan 线路生效；留空按线路名生成稳定的本地管理 MAC。');

		o = sl.option(form.Value, 'username', _('用户名'));
		o.rmempty = true;
		o.editable = true;
		o.placeholder = _('留空继承主账号');

		o = sl.option(form.Value, 'password', _('密码'));
		o.password = true;
		o.rmempty = true;
		o.modalonly = true;
		o.placeholder = _('留空继承主密码');

		o = sl.option(form.Value, 'dhcp_script', _('DHCP 脚本'));
		o.rmempty = true;
		o.modalonly = true;
		o.placeholder = 'udhcpc -i "$AUTHEAP_IFNAME" -q -n';
		o.description = _('留空继承主配置；脚本可用 $AUTHEAP_IFNAME 获取当前线路接口。');

		this._map = m;
		this._enabledOpt = s.getOption('enabled');

		var staleNotice = null;

		return m.render().then(function (node) {
			// 如果后端配置文件比 UCI 旧（例如上一次保存后没有重启服务），
			// 在页面顶部给出提示并允许一键重新生成，避免“配置改了但后端还用旧文件”。
			if (self._status && truthy(self._status.config_stale)) {
				var btn = E('button', {
					'class': 'cbi-button cbi-button-action',
					'click': function () {
						btn.disabled = true;
						callGenConfig().then(function (r) {
							if (!r || !truthy(r.res))
								throw new Error((r && (r.error || r.output)) || _('gen-config 失败'));
							return callStatus();
						}).then(function (st) {
							// 服务在跑的话，仅重写文件不会让进程用上新配置，必须重启。
							if (st && truthy(st.running))
								return callControl('restart').then(function (r) {
									if (r && truthy(r.res))
										return r;
									throw new Error((r && r.output) || _('重启服务失败'));
								});
						}).then(function () {
							ui.addNotification(null, E('p', {}, _('后端配置已重新生成并应用。')), 'info');
							window.setTimeout(function () { location.reload(); }, 800);
						}).catch(function (e) {
							ui.addNotification(null, E('p', {},
								_('重新生成失败：') + (e.message || e)), 'warning');
							btn.disabled = false;
						});
					}
				}, _('重新生成并应用'));
				staleNotice = notice('warning', [
					_('后端配置文件 /etc/autheap-client.conf 比 UCI 旧，当前运行的进程可能仍在使用旧配置。'),
					' ',
					btn
				]);
				node.insertBefore(staleNotice, node.firstChild);
			}

			// ---- 后端配置文件（只读查看）----
			var elContent = E('pre', { 'class': 'autheap-log' },
				self._configFile && self._configFile.content
					? self._configFile.content
					: _('（暂无配置文件，点“重新生成”创建）'));
			var elFileMeta = E('em', { 'class': 'cbi-value-description' },
				_('路径：') + ((self._configFile && self._configFile.path) || '/etc/autheap-client.conf') +
				((self._configFile && truthy(self._configFile.stale)) ? _('（比 UCI 旧）') : ''));

			function refreshFileView() {
				return callConfigFile().then(function (r) {
					self._configFile = r || null;
					elContent.textContent = (r && r.content) ? r.content : _('（暂无配置文件，点“重新生成”创建）');
					elFileMeta.textContent = _('路径：') + ((r && r.path) || '/etc/autheap-client.conf') +
						(r && truthy(r.stale) ? _('（比 UCI 旧）') : '');
				}, function (e) {
					ui.addNotification(null, E('p', {}, _('读取后端配置失败：') + (e.message || e)), 'warning');
				});
			}

			function regenFileView() {
				return callGenConfig().then(function (r) {
					if (r && truthy(r.res)) {
						if (staleNotice)
							staleNotice.style.display = 'none';
						ui.addNotification(null, E('p', {}, _('后端配置已重新生成。')), 'info');
						return refreshFileView();
					}
					ui.addNotification(null, E('p', {},
						_('重新生成失败：') + ((r && (r.error || r.output)) || '')), 'warning');
				}, function (e) {
					ui.addNotification(null, E('p', {}, _('重新生成失败：') + (e.message || e)), 'warning');
				});
			}

			var fileSection = E('div', { 'class': 'cbi-section' }, [
				E('h3', {}, _('后端配置')),
				E('div', { 'class': 'cbi-section-descr' },
					_('这是后端进程实际读取的配置文件，由 UCI 自动渲染；仅供查看与重新生成。')),
				E('div', { 'class': 'cbi-section-node' }, [
					E('div', { 'class': 'cbi-value' }, [
						E('label', { 'class': 'cbi-value-title' }, _('配置文件')),
						E('div', { 'class': 'cbi-value-field' }, [
							elFileMeta,
							E('div', { 'class': 'autheap-inline', 'style': 'margin:6px 0' }, [
								E('button', {
									'class': 'cbi-button cbi-button-neutral',
									'click': refreshFileView
								}, _('刷新')),
								E('button', {
									'class': 'cbi-button cbi-button-action',
									'click': regenFileView
								}, _('重新生成'))
							]),
							elContent
						])
					])
				])
			]);
			// 把只读配置文件区放在 form.Map 之外：普通“保存”后 Map 会重新渲染，
			// 放在里面会被一起清掉。
			return E('div', {}, [ viewStyle(), node, fileSection ]);
		});
	},

	// 保存表单（“保存”按钮）：除 UCI 外，把旧布尔插件位与新 plugins 列表保持一致。
	// 这样即使用户把插件列表清空，gen-config 的旧配置回退逻辑也不会把 rjv3 又加回来。
	handleSave: function (ev) {
		var self = this;

		if (!this._map)
			return this.super('handleSave', [ev]);

		return this._map.save(function () {
			// 旧配置没有 plugins 选项且用户没改插件列表时，form 不会写 UCI；
			// 此时要沿用页面显示的旧插件列表，不能把它当成“空列表”而误关插件。
			// 但如果用户确实把旧列表清空了（DynamicList 的值为空，且因为 UCI
			// 里本来就没有该选项而不会产生写入），则必须以空列表为准。
			var raw = uci.get('autheap-client', 'main', 'plugins');
			var uiVal = self._pluginsOpt ? self._pluginsOpt.formvalue('main') : null;
			var uiList = (uiVal == null) ? null : asList(uiVal);
			var list;
			if (raw == null && self._legacyPlugins && self._legacyPlugins.length) {
				list = (uiList && uiList.length === 0) ? [] : self._legacyPlugins.slice();
			}
			else {
				list = asList(raw);
			}

			// 保存后 form 会重新渲染；旧配置的 cfgvalue 仍会回退到 _legacyPlugins，
			// 因此这里同步更新它，否则刚清空的旧插件列表会在界面上“复活”。
			self._legacyPlugins = list.slice();

			// 只在值确实不同时才写 UCI，避免“打开配置页点保存”也产生
			// 一堆无意义的 changes（那会让保存按钮一直显示未应用）。
			function setIfChanged(opt, want, dflt) {
				var cur = uci.get('autheap-client', 'main', opt);
				if (cur == null)
					cur = dflt;
				if (String(cur) != String(want))
					uci.set('autheap-client', 'main', opt, String(want));
			}

			[ [ 'plugin_rjv3', 'rjv3', '1' ], [ 'plugin_printer', 'printer', '0' ], [ 'plugin_stats', 'stats', '0' ] ]
				.forEach(function (kv) {
					setIfChanged(kv[0], list.indexOf(kv[1]) >= 0 ? '1' : '0', kv[2]);
				});

			// 开机自启：以表单值回写 UCI；保存并应用后由 rpcd 的 control
			// 把它同步到 /etc/rc.d 软链（关闭自启不会停止当前进程）。
			var boot = self._enabledOpt
				? self._enabledOpt.formvalue('main')
				: (uci.get('autheap-client', 'main', 'enabled') == '1');
			self._pendingEnabled = truthy(boot);
			setIfChanged('enabled', self._pendingEnabled ? '1' : '0', '0');
		});
	},

	// 保存并应用：先走 LuCI 标准 UCI 保存/应用流程，等配置真正 commit 后再
	// 1) 重新生成后端配置文件；2) 同步开机自启软链；3) 服务在运行则重启。
	// 这里不再把“开机自启”与“停止服务”绑定：关闭自启只取消开机启动，不杀进程。
	handleSaveApply: function (ev, mode) {
		var self = this;
		var done = false;
		var guard = null;

		function afterApply() {
			if (done) return;
			done = true;
			if (guard) { window.clearTimeout(guard); guard = null; }

			var boot = (typeof self._pendingEnabled === 'boolean')
				? self._pendingEnabled
				: (uci.get('autheap-client', 'main', 'enabled') == '1');

			function checkResult(r, what) {
				if (r && truthy(r.res))
					return r;
				throw new Error((r && (r.error || r.output)) || (what + _('失败')));
			}

			callGenConfig()
				.catch(function () { /* 后面 control 里 init.d 还会再生成一次 */ })
				.then(function () {
					return callControl(boot ? 'enable' : 'disable');
				})
				.then(function (r) { return checkResult(r, _('同步开机自启')); })
				.then(function () {
					return callStatus();
				})
				.then(function (st) {
					if (st && truthy(st.running))
						return callControl('restart').then(function (r) {
							return checkResult(r, _('重启服务'));
						});
				})
				.then(function () {
					ui.addNotification(null, E('p', {},
						_('配置已保存并应用；后端配置已同步。')), 'info');
				})
				.catch(function (e) {
					// 配置本身已经应用成功，只是后端同步/重启失败；提示用户但不回滚 UCI。
					ui.addNotification(null, E('p', {},
						_('配置已保存，但同步后端服务失败：') + (e.message || e) +
						_('。可稍后在状态页手动重启服务。')), 'warning');
				});
		}

		document.addEventListener('uci-applied', afterApply, { once: true });
		// “没有实际改动”时 LuCI 不会派发 uci-applied；兜底清理监听，避免多次点击后
		// 累积多个 afterApply（下一次 apply 会触发好几次重启）。
		guard = window.setTimeout(function () {
			if (!done)
				document.removeEventListener('uci-applied', afterApply);
		}, 15000);

		return Promise.resolve(this.super('handleSaveApply', [ev, mode])).catch(function (e) {
			document.removeEventListener('uci-applied', afterApply);
			if (guard) { window.clearTimeout(guard); guard = null; }
			throw e;
		});
	}
});
