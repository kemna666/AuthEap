# OpenWrt feed/SDK package Makefile for AuthEAP-Client (Rust).
#
# OpenWrt 系统软件包名：AuthEAP-Client
# 内部标识（二进制/服务/UCI）：autheap-client
#
# 由于 OpenWrt SDK 通常不自带 Rust 工具链，推荐两条路径任选其一：
#
#   A) 用 packaging/openwrt/scripts/cross-build.sh 在宿主机交叉编译，
#      把产物放到 out/ 并设置 AUTHEAP_BIN（本 Makefile 会在编译阶段拷贝）。
#
#   B) 在 SDK 内安装 Rust 工具链后直接 cargo build（把下面的注释实现启用）。
#
# 目标系统决定产出 .ipk(≤23.05/opkg) 或 .apk(24.10+，默认)。

include $(TOPDIR)/rules.mk

PKG_NAME:=AuthEAP-Client
PKG_VERSION:=0.1.42
PKG_RELEASE:=1

PKG_LICENSE:=GPL-3.0
PKG_LICENSE_FILES:=LICENSE
PKG_MAINTAINER:=AuthEAP-Client contributors

# 预编译二进制路径（方案 A）。也可写为 feeds 远程源码包。
AUTHEAP_BIN?=$(CURDIR)/out/autheap
# 目标三元组产物（若用脚本生成到 ./staging 时取消注释下面一行）
# AUTHEAP_BIN?=$(CURDIR)/staging/autheap

include $(INCLUDE_DIR)/package.mk

define Package/AuthEAP-Client
	SECTION:=net
	CATEGORY:=Network
	TITLE:=AuthEAP-Client - 802.1x client with Ruijie v3/v4 support (Rust)
	URL:=https://github.com/updateing/minieap
	DEPENDS:=+libc
endef

define Package/AuthEAP-Client/description
	AuthEAP-Client：可扩展的 802.1x (EAP-MD5) 客户端（锐捷 v3/v4 插件支持）。Rust 版。
endef

# 方案 A：直接安装预编译产物
define Build/Compile
	@test -x "$(AUTHEAP_BIN)" || { \
		echo "缺少预编译二进制：$(AUTHEAP_BIN)"; \
		echo "请先运行 packaging/openwrt/scripts/cross-build.sh 或设置 AUTHEAP_BIN"; \
		exit 1; }
endef

define Package/AuthEAP-Client/install
	$(INSTALL_DIR) $(1)/usr/bin $(1)/etc/init.d $(1)/etc/config
	$(INSTALL_DIR) $(1)/usr/libexec/autheap-client
	$(INSTALL_BIN) $(AUTHEAP_BIN) $(1)/usr/bin/autheap
	$(INSTALL_BIN) ./root/etc/init.d/autheap-client $(1)/etc/init.d/autheap-client
	$(INSTALL_DATA) ./root/etc/config/autheap-client $(1)/etc/config/autheap-client
	$(INSTALL_BIN) ./root/usr/libexec/autheap-client/gen-config \
		$(1)/usr/libexec/autheap-client/gen-config
endef

$(eval $(call BuildPackage,AuthEAP-Client))
