include $(TOPDIR)/rules.mk

PKG_NAME:=at-webserver
PKG_VERSION:=0.1.0
PKG_RELEASE:=1

PKG_BUILD_DEPENDS:=rust/host
PKG_BUILD_DIR:=$(BUILD_DIR)/$(PKG_NAME)-$(PKG_VERSION)
PKG_BUILD_PARALLEL:=1

include $(INCLUDE_DIR)/package.mk
# Include the rust-package.mk if available. If not, one might need to adjust this.
# This assumes the 'rust' feed is installed.
include $(TOPDIR)/feeds/packages/lang/rust/rust-package.mk

define Package/at-webserver
  SECTION:=net
  CATEGORY:=Network
  TITLE:=AT Command WebSocket Server (Rust)
  DEPENDS:=$(RUST_ARCH_DEPENDS)
endef

define Package/at-webserver/description
  Rust implementation of AT Webserver for OpenWrt.
  Provides WebSocket interface for AT commands.
endef

	mkdir -p $(PKG_BUILD_DIR)
	$(CP) ./src $(PKG_BUILD_DIR)/
	$(CP) ./files $(PKG_BUILD_DIR)/
	$(CP) ./Cargo.toml $(PKG_BUILD_DIR)/
	$(CP) ./Cargo.lock $(PKG_BUILD_DIR)/
endef

define Package/at-webserver/install
	$(INSTALL_DIR) $(1)/usr/bin
	# rust-package.mk installs binary to $(PKG_INSTALL_DIR)/bin, we copy it from there.
	# Note: Check if rust-package.mk installs to /usr/bin or /bin. Usually it respects Cargo bin output.
	# We assume the binary name matches the package name or is defined in Cargo.toml.
	# The Cargo.toml name is "at-webserver".
	$(INSTALL_BIN) $(PKG_INSTALL_DIR)/bin/at-webserver $(1)/usr/bin/
	
	$(INSTALL_DIR) $(1)/etc/init.d
	$(INSTALL_BIN) ./files/etc/init.d/at-webserver $(1)/etc/init.d/

	$(INSTALL_DIR) $(1)/etc/config
	$(INSTALL_CONF) ./files/etc/config/at-webserver $(1)/etc/config/

	$(INSTALL_DIR) $(1)/www/5700
	$(CP) -r ./files/www/5700/* $(1)/www/5700/
	
	$(INSTALL_DIR) $(1)/www/cgi-bin
	$(INSTALL_BIN) ./files/www/cgi-bin/at-ws-info $(1)/www/cgi-bin/
	$(INSTALL_BIN) ./files/www/cgi-bin/at-log-clear $(1)/www/cgi-bin/
endef

$(eval $(call RustBinPackage,at-webserver))
$(eval $(call BuildPackage,at-webserver))
