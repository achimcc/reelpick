{ pkgs, module, package }:
let
  fake = pkgs.writeText "fake.py" ''
    import json
    from http.server import BaseHTTPRequestHandler, HTTPServer

    LIB = "f137a2dd21bbc1b99aa5c0f6bf02a805"
    FILM = {"Name": "Heat", "Id": "abc", "ProductionYear": 1995, "ProviderIds": {"Tmdb": "949"},
            "Genres": ["Crime"], "CommunityRating": 7.9, "Overview": "Two men. One city.",
            "RunTimeTicks": 102000000000, "People": [{"Name": "Michael Mann", "Type": "Director"}]}
    CHOICE = {"choice": 949, "reason": "Because.", "teaser": "Two men on either side of the law, and one city between them.",
              "article": "### Tonight\n\n" + "A sentence about the film. " * 40}

    class H(BaseHTTPRequestHandler):
        def _json(self, obj):
            body = json.dumps(obj).encode()
            self.send_response(200); self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body)
        def do_GET(self):
            if self.headers.get("Authorization") != 'MediaBrowser Token="vm-key"':
                self.send_response(401); self.end_headers(); return
            if self.path.startswith("/Library/MediaFolders"):
                self._json({"Items": [{"Name": "Filme", "Id": LIB, "CollectionType": "movies"}]})
            elif self.path.startswith("/Items/abc/Images/Primary"):
                self.send_response(404); self.end_headers()
            elif self.path.startswith("/Items"):
                self._json({"Items": [FILM], "TotalRecordCount": 1})
            else:
                self.send_response(404); self.end_headers()
        def do_POST(self):
            length = int(self.headers.get("Content-Length", 0)); self.rfile.read(length)
            self._json({"done": True, "message": {"role": "assistant", "content": json.dumps(CHOICE)}})
        def log_message(self, *a): pass

    HTTPServer(("127.0.0.1", 18096), H).serve_forever()
  '';
in
pkgs.testers.runNixOSTest {
  name = "reelpick";

  nodes.machine =
    { ... }:
    {
      imports = [ module ];
      time.timeZone = "UTC";
      environment.systemPackages = [ pkgs.sqlite ];

      systemd.services.fake = {
        wantedBy = [ "multi-user.target" ];
        serviceConfig.ExecStart = "${pkgs.python3}/bin/python3 ${fake}";
      };

      environment.etc."reelpick-jellyfin-key".text = "vm-key\n";

      services.reelpick = {
        enable = true;
        inherit package;
        jellyfinApiKeyFile = "/etc/reelpick-jellyfin-key";
        settings = {
          listen = "127.0.0.1:8080";
          base_path = "/reelpick";
          language = "en";
          timezone = "Europe/Berlin";
          jellyfin = {
            url = "http://127.0.0.1:18096";
            public_url = "https://jellyfin.example.org";
            library = "Filme";
          };
          ollama = {
            url = "http://127.0.0.1:18096";
            model = "qwen3:8b";
          };
        };
      };
    };

  testScript = ''
    machine.wait_for_unit("fake.service")
    machine.wait_for_unit("reelpick.service")
    machine.wait_for_open_port(8080)

    with subtest("without a pick the fragment is quiet and healthz says so"):
        machine.succeed("curl -fsS http://127.0.0.1:8080/reelpick/today.html | grep -q reelpick-empty")
        machine.fail("curl -fsS http://127.0.0.1:8080/reelpick/healthz")

    with subtest("the timer is armed"):
        machine.succeed("systemctl is-active reelpick-pick.timer")

    with subtest("the key is a credential of the pick unit and not in its environment"):
        # `systemctl show -p LoadCredential` redacts the value as "[unprintable]"
        # (systemd treats LoadCredential as sensitive); `systemctl cat` still
        # renders the actual unit file, so use that to see the credential spec.
        machine.succeed("systemctl cat reelpick-pick.service | grep -q 'LoadCredential=jellyfin:'")
        machine.fail("systemctl show reelpick-pick.service -p Environment | grep -q vm-key")
        machine.fail("systemctl show reelpick.service -p LoadCredential | grep -q jellyfin")

    with subtest("a pick runs, once"):
        machine.succeed("systemctl start reelpick-pick.service")
        machine.succeed("curl -fsS http://127.0.0.1:8080/reelpick/today.html | grep -q Heat")
        date = machine.succeed("curl -fsS http://127.0.0.1:8080/reelpick/healthz").strip()
        machine.succeed("systemctl start reelpick-pick.service")
        machine.succeed(f"journalctl -u reelpick-pick.service | grep -q '{date} already has its pick'")
        machine.succeed(f"curl -fsS http://127.0.0.1:8080/reelpick/{date} | grep -q 'jellyfin.example.org/web/index.html#/details?id=abc'")
        machine.succeed("test $(sqlite3 /var/lib/reelpick/reelpick.db 'select count(*) from picks') = 1")
  '';
}
