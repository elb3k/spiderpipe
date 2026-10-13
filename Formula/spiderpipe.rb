class Spiderpipe < Formula
  desc "Stream whatever is piped into stdin to a live web page"
  homepage "https://github.com/elb3k/spiderpipe"
  url "https://github.com/elb3k/spiderpipe/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "REPLACE_WITH_SHA256_OF_THE_TARBALL"
  head "https://github.com/elb3k/spiderpipe.git", branch: "master"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/spiderpipe --version")

    port = free_port
    pid = spawn "sh", "-c", "echo hello from brew | #{bin}/spiderpipe -p #{port}"
    sleep 2
    assert_match "hello from brew", shell_output("curl -s --max-time 2 http://127.0.0.1:#{port}/raw")
  ensure
    Process.kill("TERM", pid) if pid
  end
end
