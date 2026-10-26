# 前端开发服务器：静态托管 frontend/ 并禁用缓存（浏览器 Mock 模式调试用）
# 用法：python scripts/serve_dev.py [端口]
import http.server
import os
import sys

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8124
ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "frontend")


class NoCacheHandler(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header("Cache-Control", "no-store, must-revalidate")
        self.send_header("Pragma", "no-cache")
        super().end_headers()

    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=ROOT, **kwargs)


if __name__ == "__main__":
    print(f"Serving {os.path.abspath(ROOT)} at http://127.0.0.1:{PORT} (no-cache)")
    http.server.ThreadingHTTPServer(("127.0.0.1", PORT), NoCacheHandler).serve_forever()
