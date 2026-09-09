# OpenWrt package: luci-app-autheap-client (new JS + ucode, OpenWrt 24.10+)
#
# 需要先安装主程序包 AuthEAP-Client。安装后重启 rpcd/luci：
#   /etc/init.d/rpcd restart; /etc/init.d/luci reload

include $(TOPDIR)/rules.mk

LUCI_TITLE:=LuCI support for AuthEAP-Client (802.1x client)
LUCI_DEPENDS:=+AuthEAP-Client +luci-base +rpcd-mod-rpcsys +ucode-mod-system +ucode-mod-uci +ucode-mod-fs
LUCI_PKGARCH:=all

include $(TOPDIR)/feeds/luci/luci.mk

# 默认 luci.mk 的 Package/luci-app-autheap-client/install 会拷贝 ./root 内容；
# 本包文件放在 ./root 下，无需额外自定义。
define Package/luci-app-autheap-client/install
	$(INSTALL_DIR) $(1)
	$(CP) ./root/* $(1)/
endef

$(eval $(call BuildPackage,luci-app-autheap-client))
