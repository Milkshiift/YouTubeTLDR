{
  description = "YouTubeTLDR server";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
    crane.inputs.nixpkgs.follows = "nixpkgs";
    fenix = {
        url = "github:nix-community/fenix";
        inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, fenix, crane, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
        };

        rust-toolchain = fenix.packages.${system}.default.toolchain;
        craneLib = (crane.mkLib pkgs).overrideToolchain rust-toolchain;

        project = pkgs.lib.importTOML ./Cargo.toml;

        youtubetldr = craneLib.buildPackage {
          pname = project.package.name;
          version = project.package.version;
          src = ./.;
          nativeBuildInputs = with pkgs; [
            mold
          ];
          cargoExtraArgs = "--no-default-features --features rustls-tls";
        };
      in
      {
        packages.default = youtubetldr;

        devShells = {
          default = pkgs.mkShell {
            buildInputs = with pkgs; [
              openssl
              pkg-config
            ];
          };
        };
      }) // {
        nixosModules.default = { config, pkgs, ... }:
          let
            cfg = config.services.youtubetldr;
          in
          {
            options.services.youtubetldr = {
              enable = pkgs.lib.mkEnableOption "Enable the YouTubeTLDR server";

              ip = pkgs.lib.mkOption {
                type = pkgs.lib.types.str;
                default = "0.0.0.0";
                description = "IP address to bind to";
              };

              port = pkgs.lib.mkOption {
                type = pkgs.lib.types.port;
                default = 8000;
                description = "Port to listen on";
              };

              workers = pkgs.lib.mkOption {
                type = pkgs.lib.types.int;
                default = 4;
                description = "Number of worker threads";
              };
            };

            config = pkgs.lib.mkIf cfg.enable {
              systemd.services.youtubetldr = {
                description = "YouTubeTLDR server";
                after = [ "network.target" ];
                wantedBy = [ "multi-user.target" ];

                serviceConfig = {
                  ExecStart = "${self.packages.${pkgs.system}.default}/bin/YouTubeTLDR";
                  Restart = "on-failure";
                  User = "youtubetldr";
                  Group = "youtubetldr";
                  CapabilityBoundingSet = ""; # No capabilities
                  PrivateTmp = true; # Private /tmp and /var/tmp
                  NoNewPrivileges = true; # Prevent privilege escalation
                  ProtectSystem = "strict"; # Protect the /boot, /etc, /usr, and /opt hierarchies
                  ProtectHome = true; # Protect /home, /root, /run/user
                  RestrictSUIDSGID = true; # Prevent SUID/SGID bits from being set
                  RestrictRealtime = true; # Prevent realtime scheduling
                  RestrictAddressFamilies = [ "AF_INET" "AF_INET6" ]; # Only allow IPv4 and IPv6 sockets
                  SystemCallFilter = [ "~@cpu-emulation @debug @keyring @mount @module @obsolete @raw-io @reboot @swap" ]; # Restrict syscalls
                  Environment = [
                    "TLDR_IP=${cfg.ip}"
                    "TLDR_PORT=${toString cfg.port}"
                    "TLDR_WORKERS=${toString cfg.workers}"
                  ];
                };
              };

              users.users.youtubetldr = {
                isSystemUser = true;
                group = "youtubetldr";
              };

              users.groups.youtubetldr = {};
            };
          };
      };
}