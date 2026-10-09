import json
import os
from pathlib import Path
import re
import select
import subprocess
import time
import unittest
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen


class TicketProcessTest(unittest.TestCase):
    def setUp(self):
        self.processes = []

    def tearDown(self):
        for process in reversed(self.processes):
            self.stop_server(process)

    def test_real_jar_lifecycle_and_error_contract(self):
        process, base_url = self.start_server()
        status, health = self.request_json(base_url + "/api/health")
        self.assertEqual(status, 200)
        self.assertEqual(health, {"status": "ok"})

        status, created = self.request_json(
            base_url + "/api/tickets",
            method="POST",
            body={"customer_id": 42, "title": "  进程验收  "},
        )
        self.assertEqual(status, 201)
        self.assertEqual(set(created), {"ticket_id", "customer_id", "title", "status"})
        self.assertEqual(created["customer_id"], 42)
        self.assertEqual(created["title"], "进程验收")
        self.assertEqual(created["status"], "OPEN")
        ticket_id = created["ticket_id"]

        status, read = self.request_json(base_url + "/api/tickets/" + ticket_id)
        self.assertEqual(status, 200)
        self.assertEqual(read, created)

        status, error = self.request_json(
            base_url + "/api/tickets",
            method="POST",
            body={"customer_id": "42", "title": "错误类型"},
        )
        self.assertEqual(status, 400)
        self.assertEqual(error["code"], "BAD_REQUEST")

        self.stop_server(process)
        self.processes.remove(process)

        restarted, restarted_base_url = self.start_server()
        status, error = self.request_json(restarted_base_url + "/api/tickets/" + ticket_id)
        self.assertEqual(status, 404)
        self.assertEqual(error["code"], "NOT_FOUND")

    def start_server(self):
        jar = self.find_jar()
        java = str(Path(os.environ.get("JAVA_HOME", "")) / "bin" / "java")
        if not Path(java).is_file():
            java = "java"
        process = subprocess.Popen(
            [java, "-jar", str(jar), "--server.port=0", "--server.address=127.0.0.1"],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            bufsize=1,
        )
        self.processes.append(process)
        log = []
        port = None
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            if process.poll() is not None:
                break
            readable, _, _ = select.select([process.stdout], [], [], 0.1)
            if readable:
                line = process.stdout.readline()
                if not line:
                    break
                log.append(line)
                match = re.search(r"Tomcat started on port (\d+)", line)
                if match:
                    port = int(match.group(1))
            if port is not None:
                try:
                    status, _ = self.request_json(f"http://127.0.0.1:{port}/api/health")
                    if status == 200:
                        return process, f"http://127.0.0.1:{port}"
                except (HTTPError, URLError, TimeoutError):
                    pass
        output = "".join(log)
        self.stop_server(process)
        self.processes.remove(process)
        self.fail("Java 进程未在限定时间内就绪；输出：\n" + output)

    def stop_server(self, process):
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
        if process.stdout is not None:
            process.stdout.close()

    def find_jar(self):
        project = Path(__file__).resolve().parents[3]
        jars = sorted(
            path
            for path in (project / "target").glob("*.jar")
            if not path.name.startswith("original-")
        )
        if not jars:
            self.fail("target/ 下未找到 Maven 打包的 JAR")
        return jars[-1]

    def request_json(self, url, method="GET", body=None):
        data = None if body is None else json.dumps(body, ensure_ascii=False).encode("utf-8")
        headers = {} if body is None else {"Content-Type": "application/json"}
        request = Request(url, data=data, headers=headers, method=method)
        try:
            with urlopen(request, timeout=10) as response:
                return response.status, json.loads(response.read().decode("utf-8"))
        except HTTPError as error:
            return error.code, json.loads(error.read().decode("utf-8"))


if __name__ == "__main__":
    unittest.main()
