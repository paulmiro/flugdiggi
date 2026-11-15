import asyncio
import json
import os

import decky

HELPER_TIMEOUT = 90


class Plugin:
    async def _main(self):
        self.lock = asyncio.Lock()
        self.helper = os.path.join(decky.DECKY_PLUGIN_DIR, "bin", "flugdiggi-decky")
        self.tasks = set()

    async def _unload(self):
        for task in self.tasks:
            task.cancel()

    async def _run(self, *args, stdin=None):
        # One USB operation at a time.
        async with self.lock:
            try:
                proc = await asyncio.create_subprocess_exec(
                    self.helper,
                    *args,
                    stdin=asyncio.subprocess.PIPE,
                    stdout=asyncio.subprocess.PIPE,
                    stderr=asyncio.subprocess.PIPE,
                )
                out, err = await asyncio.wait_for(proc.communicate(stdin), timeout=HELPER_TIMEOUT)
            except asyncio.TimeoutError:
                proc.kill()
                return self._failure("Timeout", "the helper did not finish in time")
            except OSError as e:
                return self._failure("Helper", f"cannot run the helper: {e}")
        if err:
            decky.logger.info(err.decode(errors="replace").strip())
        try:
            return json.loads(out)
        except json.JSONDecodeError:
            return self._failure("Helper", f"the helper crashed (exit code {proc.returncode})")

    @staticmethod
    def _failure(kind, message):
        return {"ok": False, "error": {"kind": kind, "message": message}}

    async def meta(self):
        return await self._run("meta")

    async def list_devices(self):
        return await self._run("list")

    async def status(self, port):
        return await self._run("status", "--port", port)

    async def read_buttons(self, port, slot):
        return await self._run("read-buttons", "--port", port, "--slot", str(slot))

    async def activate(self, port, slot):
        return await self._run("activate", "--port", port, "--slot", str(slot))

    # A write takes several seconds: return at once and report through the write_done event.
    async def write_buttons(self, port, slot, buttons, expected_version):
        task = asyncio.get_running_loop().create_task(self._write(port, slot, buttons, expected_version))
        self.tasks.add(task)
        task.add_done_callback(self.tasks.discard)

    async def _write(self, port, slot, buttons, expected_version):
        await decky.emit("write_started", slot)
        request = json.dumps({"buttons": buttons, "expected_data_version": expected_version}).encode()
        result = await self._run(
            "write-buttons",
            "--port", port,
            "--slot", str(slot),
            "--backup-dir", os.path.join(decky.DECKY_PLUGIN_RUNTIME_DIR, "backups"),
            stdin=request,
        )
        await decky.emit("write_done", slot, result)
