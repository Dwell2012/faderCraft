"""Device-selection and retry checks; no model download or GPU required."""
import unittest
import io
import json
from unittest.mock import Mock, patch

import torch
import prepare_stems as worker


class DeviceTests(unittest.TestCase):
    def test_cpu_override_never_probes_gpu(self):
        with patch.object(torch.cuda, "is_available") as probe:
            self.assertEqual(worker.select_device("cpu"), "cpu")
            probe.assert_not_called()

    def test_automatic_uses_cuda_when_probe_succeeds(self):
        with patch.object(torch.cuda, "is_available", return_value=True), \
             patch.object(torch, "empty"), \
             patch.object(torch.cuda, "get_device_name", return_value="test GPU"):
            self.assertEqual(worker.select_device("auto"), "cuda")

    def test_failed_gpu_probe_falls_back(self):
        with patch.object(torch.cuda, "is_available", return_value=True), \
             patch.object(torch, "empty", side_effect=RuntimeError("driver unavailable")):
            self.assertEqual(worker.select_device("auto"), "cpu")

    def test_missing_gpu_uses_cpu(self):
        with patch.object(torch.cuda, "is_available", return_value=False), \
             patch.object(torch.backends.mps, "is_available", return_value=False):
            self.assertEqual(worker.select_device("auto"), "cpu")

    def test_gpu_error_retries_once_on_cpu(self):
        model = Mock()
        wave = torch.zeros(1, 2, 128)
        output = torch.zeros(1, 4, 2, 128)
        with patch.object(worker, "apply_model", side_effect=[RuntimeError("out of memory"), output]) as apply, \
             patch.object(torch.cuda, "empty_cache"):
            result = worker.separate(model, wave, "cuda")
            self.assertEqual([call.kwargs["device"] for call in apply.call_args_list], ["cuda", "cpu"])
            self.assertEqual(tuple(result.shape), (4, 2, 128))
            model.cpu.assert_called_once()

    def test_cpu_errors_are_not_silently_retried(self):
        with patch.object(worker, "apply_model", side_effect=RuntimeError("bad input")) as apply:
            with self.assertRaisesRegex(RuntimeError, "bad input"):
                worker.separate(Mock(), torch.zeros(1, 2, 128), "cpu")
            self.assertEqual(apply.call_count, 1)


class ServerTests(unittest.TestCase):
    def test_one_model_is_warmed_then_reused_after_job_error_and_device_change(self):
        model = Mock(samplerate=44100)
        model.cpu.return_value.eval.return_value.samplerate = 44100
        requests = [dict(input="in", output="out", sample_rate=44100,
                         device=device, log="log") for device in ["auto", "cpu", "auto"]]
        output = io.StringIO()
        with patch.object(worker, "load_model", return_value=model.cpu.return_value.eval.return_value) as load, \
             patch.object(worker, "select_device", side_effect=lambda p: "cuda" if p == "auto" else "cpu"), \
             patch.object(worker, "separate") as warm, \
             patch.object(worker, "prepare", side_effect=[ValueError("bad track"), None, None]) as prepare, \
             patch("builtins.open", unittest.mock.mock_open()), \
             patch.object(worker.sys, "stdin", io.StringIO("".join(json.dumps(r) + "\n" for r in requests))), \
             patch.object(worker.sys, "stdout", output), \
             patch.object(worker.sys, "stderr", io.StringIO()):
            cached = model.cpu.return_value.eval.return_value
            worker.serve("auto")
            load.assert_called_once_with("htdemucs")
            warm.assert_called_once()
            self.assertEqual(prepare.call_count, 3)
            self.assertTrue(all(c.args[4] is cached for c in prepare.call_args_list))
            self.assertEqual([c.args[5] for c in prepare.call_args_list], ["cuda", "cpu", "cuda"])
        self.assertEqual(output.getvalue().splitlines(),
                         ["DJCRAFT_READY", "DJCRAFT_ERROR", "DJCRAFT_DONE", "DJCRAFT_DONE"])


    def test_switching_models_loads_the_requested_weights(self):
        fast, detailed, fast_again = Mock(samplerate=44100), Mock(), Mock()
        requests = [dict(input="in", output="out", sample_rate=44100, device="cpu",
                         model=name, log="log") for name in ["htdemucs", "htdemucs_ft", "htdemucs"]]
        with patch.object(worker, "load_model", side_effect=[fast, detailed, fast_again]) as load, \
             patch.object(worker, "select_device", return_value="cpu"), \
             patch.object(worker, "separate"), patch.object(worker, "prepare") as prepare, \
             patch("builtins.open", unittest.mock.mock_open()), \
             patch.object(worker.sys, "stdin", io.StringIO("".join(json.dumps(r) + "\n" for r in requests))), \
             patch.object(worker.sys, "stdout", io.StringIO()):
            worker.serve("cpu")
            self.assertEqual([c.args[0] for c in load.call_args_list], ["htdemucs", "htdemucs_ft", "htdemucs"])
            self.assertEqual([c.args[4] for c in prepare.call_args_list], [fast, detailed, fast_again])
            self.assertEqual([c.args[6] for c in prepare.call_args_list], ["htdemucs", "htdemucs_ft", "htdemucs"])
            fast.cpu.assert_called_once()
            detailed.cpu.assert_called_once()

    def test_lost_gpu_context_reloads_the_selected_model_for_cpu(self):
        model = Mock()
        model.cpu.side_effect = RuntimeError("lost context")
        with patch.object(worker, "apply_model", side_effect=[RuntimeError("lost context"), torch.zeros(1, 4, 2, 128)]), \
             patch.object(worker, "load_model", return_value=Mock()) as reload, \
             patch.object(torch.cuda, "empty_cache"):
            worker.separate(model, torch.zeros(1, 2, 128), "cuda", "htdemucs_ft")
            reload.assert_called_once_with("htdemucs_ft")


if __name__ == "__main__":
    unittest.main()
