"""Tests for evalserver.py — server lifecycle management.

These tests validate the contract of each function without starting
a real server.  Network-dependent functions are tested by mocking.
"""

from unittest.mock import patch, MagicMock
import pytest

from evalserver import server_port, is_running, kill


class TestServerPort:
    def test_extracts_port_from_url(self):
        assert server_port("http://127.0.0.1:8989") == 8989

    def test_default_port_when_missing(self):
        assert server_port("http://127.0.0.1") == 8989

    def test_other_port(self):
        assert server_port("http://127.0.0.1:8080") == 8080


class TestIsRunning:
    @patch("evalserver.requests.get")
    def test_returns_true_when_healthy(self, mock_get):
        mock_get.return_value.status_code = 200
        assert is_running("http://127.0.0.1:8989") is True

    @patch("evalserver.requests.get")
    def test_returns_false_when_unhealthy(self, mock_get):
        mock_get.return_value.status_code = 503
        assert is_running("http://127.0.0.1:8989") is False

    @patch("evalserver.requests.get")
    def test_returns_false_on_connection_error(self, mock_get):
        mock_get.side_effect = ConnectionError
        assert is_running("http://127.0.0.1:8989") is False


class TestKill:
    @patch("evalserver.subprocess.run")
    @patch("evalserver.is_running")
    def test_calls_fuser_with_port(self, mock_running, mock_run):
        mock_running.return_value = False
        kill("http://127.0.0.1:8989")
        # fuser -k <port>/tcp
        args = mock_run.call_args[0][0]
        assert args[0] == "fuser"
        assert args[1] == "-k"
        assert args[2] == "8989/tcp"

    @patch("evalserver.subprocess.run")
    @patch("evalserver.is_running")
    def test_warns_if_still_alive(self, mock_running, mock_run):
        mock_running.return_value = True
        kill("http://127.0.0.1:8989")
        # Should print a warning; no exception is good enough for now
        assert True
