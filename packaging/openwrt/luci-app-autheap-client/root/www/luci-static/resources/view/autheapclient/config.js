'use strict';
'require form';
'require uci';
'require view';
'require rpc';
'require tools.widgets as widgets';

var callListPlugins = rpc.declare({
	object: 'luci.autheap_client',
	method: 'list_plugins',
	expect: {}
});

var callControl = rpc.declare({
	object: 'luci.autheap_client',
	method: 'control',
	expect: { res: true }
});

return view.extend({
	load: function () {
		var self = this;

		return Promise.all([ uci.load('autheap-client'), callListPlugins() ])
			.then(function (r) {
				self._available = (r[1] && r[1].plugins) || [];

				// 旧配置迁移：只有布尔字段、没有 list plugins 时，把已启用的填进内存列表，
				// 让 DynamicList 能显示；用户保存后即写为新 list（init.d 优先使用 list）。
				var plugins = uci.get('autheap-client', 'main', 'plugins');
				if (plugins == null) {
					var legacy = [];
					[ [ 'plugin_rjv3', 'rjv3' ], [ 'plugin_printer', 'printer' ], [ 'plugin_stats', 'stats' ] ]
						.forEach(function (kv) {
							if (uci.get('autheap-client', 'main', kv[0]) == '1')
								legacy.push(kv[1]);
						});
					var extra = uci.get('autheap-client', 'main', 'extra_plugins');
					if (extra) {
						String(extra).split(/[\s,]+/).forEach(function (n) {
							if (n && legacy.indexOf(n) < 0) legacy.push(n);
						});
					}
					if (legacy.length)
						uci.set('autheap-client', 'main', 'plugins', legacy);
				}
			});
	},

	render: function () {
		var avail = this._available || [];
		var names = avail.map(function (p) { return p.name; }).join('、');

		var m = new form.Map('autheap-client', 'AuthEAP-Client',
			names ? ('可用插件：' + names) : '启用认证所需的插件');

		var s = m.section(form.NamedSection, 'main', 'autheap-client', '认证设置');

		var o;

		o = s.option(form.Flag, 'enabled', '开机自启（系统启动时自动运行）');

		o = s.option(form.Flag, 'daemon', '后台运行（守护模式；不交给 procd 托管，停止用 killall）');

		o = s.option(form.Flag, 'printer_log_recv', 'printer 插件：把收到的帧也打印到日志');

		o = s.option(widgets.DeviceSelect, 'ifname', '认证网卡（设备名，可从下拉选择或手动输入）');
		o.noaliases = true;
		o.rmempty = false;

		o = s.option(form.Value, 'username', '用户名');

		o = s.option(form.Value, 'password', '密码（存于 UCI，只读回显为空）');
		o.password = true;

		o = s.option(form.ListValue, 'backend', '收发包后端');
		o.value('raw', 'raw (AF_PACKET)');
		o.value('pcap', 'pcap (需 feature)');

		o = s.option(form.DynamicList, 'plugins', '启用插件（逐行添加/删除）',
			'输入要启用的插件名后回车或点“添加”；已启用项点 ✕ 移除。可用：' + names);

		o = s.option(form.Value, 'heartbeat', '心跳间隔（秒，0 关闭）');
		o.datatype = 'uinteger';

		o = s.option(form.ListValue, 'eap_bcast_addr', 'EAPOL-Start 广播地址 (0/1)');
		o.value('0', '0=标准');
		o.value('1', '1=锐捷');

		o = s.option(form.ListValue, 'dhcp_type', 'DHCP 方式');
		o.value('0', '0=不使用');
		o.value('1', '1=二次认证');
		o.value('2', '2=认证后');
		o.value('3', '3=认证前');

		o = s.option(form.Value, 'dhcp_script', 'DHCP 脚本');
		o = s.option(form.Value, 'service', '服务名');
		o = s.option(form.Value, 'version_str', '版本串');
		o = s.option(form.Value, 'fake_dns1', '主 DNS（可选）');
		o = s.option(form.Value, 'fake_dns2', '次 DNS（可选）');
		o = s.option(form.Value, 'fake_serial', '硬盘序列号（可选）');

		o = s.option(form.Value, 'max_dhcp_count', '二次认证等待 DHCP 次数上限');
		o.datatype = 'uinteger';
		o = s.option(form.Value, 'max_retries', '阶段最大重试次数');
		o.datatype = 'uinteger';
		o = s.option(form.Value, 'max_failures', '最大连续失败次数');
		o.datatype = 'uinteger';
		o = s.option(form.Value, 'stage_timeout', '阶段超时（秒）');
		o.datatype = 'uinteger';

		return m.render();
	},

	// 官方“保存并应用”：先按默认逻辑保存并应用 UCI，再据此启停服务
	handleSaveApply: function (ev, mode) {
		var Fn = L.bind(function () {
			var enabled = (uci.get('autheap-client', 'main', 'enabled') == '1');
			callControl({ action: enabled ? 'enable' : 'disable' })
				.then(function () {
					return callControl({ action: enabled ? 'restart' : 'stop' });
				})
				.catch(function () {});
			document.removeEventListener('uci-applied', Fn);
		});
		document.addEventListener('uci-applied', Fn);
		return this.super('handleSaveApply', [ev, mode]);
	}
});
