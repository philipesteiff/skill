class Skill < Formula
  desc "CLI for managing skills"
  homepage "https://github.com/philipesteiff/skill"
  url "https://github.com/philipesteiff/skill/releases/download/v0.0.3/skill-darwin-arm64.tar.gz"
  sha256 "9aeb6cec993a113bbbd5b038d7d9d49bc06324bb88ff3a1dee4cb2862a04fdbb"
  version "0.0.3"

  def install
    bin.install "skill"
  end
end
