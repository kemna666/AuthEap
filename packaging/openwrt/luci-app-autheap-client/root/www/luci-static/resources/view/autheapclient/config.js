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

// 注意：LuCI 的 `expect` 只取【第一个键】且类型不符就用默认值；
// 这里要用整个返回对象，所以不写 expect。
// 另外必须声明 params（按位置映射），否则传入的参数会被丢弃。
var callControl = rpc.declare({
	object: 'luci.autheap_client',
	method: 'control',
	params: [ 'action' ]
});

// 若报 “Access denied”，通常是升级了 luci 包但 ACL 还没生效（rpcd 未重启 / 会话缓存旧 ACL）
var ACL_HINT = '（若提示 Access denied：请在设备上执行 /etc/init.d/rpcd restart 并重新登录 LuCI，再 Ctrl+F5 强刷）';

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

				// 同步旧布尔位（plugin_rjv3/printer/stats）与插件列表：
				// 以前把列表清空后，gen-config 会回退到这些布尔值（plugin_rjv3 默认 1），
				// 于是插件“删不掉”。现在保存时会把两者写成一致。
				var cur = uci.get('autheap-client', 'main', 'plugins') || [];
				cur = String(cur).split(/[\s,]+/).filter(function (n) { return n; });
				[ [ 'plugin_rjv3', 'rjv3' ], [ 'plugin_printer', 'printer' ], [ 'plugin_stats', 'stats' ] ]
					.forEach(function (kv) {
						uci.set('autheap-client', 'main', kv[0], cur.indexOf(kv[1]) >= 0 ? '1' : '0');
					});
			});
	},

	render: function () {
		var avail = this._available || [];
		var names = avail.map(function (p) { return p.name; }).join('、');

		var m = new form.Map('autheap-client', 'AuthEAP-Client',
			names ? ('可用插件：' + names) : '启用认证所需的插件');

		// 分 Tab 显示：LuCI 的 form 支持 tabbed —— 每个 section 的【类型】即 tab 标识，
		// 【标题】即 tab 名称；各插件/功能的设置放在各自的 Tab 里。
		m.tabbed = true;

		var sBasic   = m.section(form.NamedSection, 'main', 'basic',   '基本设置');
		var sPlugins = m.section(form.NamedSection, 'main', 'plugins', '插件');
		var sRjv3    = m.section(form.NamedSection, 'main', 'rjv3',    'rjv3 锐捷',
			'锐捷 v3/v4 插件参数：心跳、DHCP、服务名/版本串、伪造字段等');
		var sBeacon  = m.section(form.NamedSection, 'main', 'beacon',  'beacon 保活');
		var sLog     = m.section(form.NamedSection, 'main', 'log',     '日志');

		var s = sBasic, o;

		o = s.option(form.Flag, 'enabled', '开机自启（系统启动时自动运行）');

		o = s.option(form.Flag, 'daemon', '后台运行（守护模式；不交给 procd 托管，停止用 killall）');

		o = s.option(widgets.DeviceSelect, 'ifname', '认证网卡（设备名，可从下拉选择或手动输入）');
		o.noaliases = true;
		o.rmempty = false;

		o = s.option(form.Value, 'username', '用户名');

		o = s.option(form.Value, 'password', '密码（存于 UCI，只读回显为空）');
		o.password = true;

		o = s.option(form.ListValue, 'backend', '收发包后端');
		o.value('raw', 'raw (AF_PACKET)');
		o.value('pcap', 'pcap (需 feature)');

		// ================= Tab: 插件 =================
		s = sPlugins;

		o = s.option(form.DynamicList, 'plugins', '启用插件（逐行添加/删除）',
			'输入要启用的插件名后回车或点“添加”；已启用项点 ✕ 移除。可用：' + names);

		o = s.option(form.Flag, 'printer_log_recv', 'printer 插件：把收到的帧也打印到日志');

		// ================= Tab: beacon 保活 =================
		// 周期性向指定 IP:端口 发送自定义 TCP 数据包；填了目标 IP + 载荷即自动启用
		s = sBeacon;

		o = s.option(form.Value, 'beacon_dst', '目标 IP / 主机名（填了就自动启用 beacon 插件）');
		o.placeholder = '例如 203.0.113.10';
		o.rmempty = true;

		o = s.option(form.Value, 'beacon_port', '目标端口（默认 8083）');
		o.datatype = 'port';
		o.placeholder = '8083';

		o = s.option(form.Value, 'beacon_interval', '发送间隔（秒；0 = 禁用）');
		o.datatype = 'uinteger';
		o.placeholder = '60';

		o = s.option(form.Value, 'beacon_payload', '载荷（十六进制；留空使用内置默认载荷）');
		o.placeholder = '留空 = 内置默认；如需覆盖请粘贴抓包 Data（偶数长度十六进制）';
		o.rmempty = true;

		// ================= Tab: 日志 =================
		// 实时刷新 + 按大小自动轮转清理
		s = sLog;

		o = s.option(form.Value, 'log_file', '日志文件（后端写入该文件）');
		o.placeholder = '/var/log/autheap-client.log';
		o.rmempty = true;

		o = s.option(form.Value, 'log_max_kb', '日志大小上限（KB；超过就自动轮转清理）');
		o.datatype = 'uinteger';
		o.placeholder = '256';

		o = s.option(form.Value, 'log_keep', '保留历史日志份数（.1/.2…；0 = 不保留）');
		o.datatype = 'uinteger';
		o.placeholder = '3';

		o = s.option(form.Value, 'log_rotate_secs', '日志检查/自动清理间隔（秒）');
		o.datatype = 'uinteger';
		o.placeholder = '300';

		// ================= Tab: rjv3 锐捷 =================
		s = sRjv3;

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

		// ================= Tab: 基本设置（续）=================
		s = sBasic;

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
			callControl(enabled ? 'enable' : 'disable')
				.then(function () {
					return callControl(enabled ? 'restart' : 'stop');
				})
				.catch(function () {});
			document.removeEventListener('uci-applied', Fn);
		});
		document.addEventListener('uci-applied', Fn);
		return this.super('handleSaveApply', [ev, mode]);
	}
});
