class Skill < Formula
  desc "CLI for managing skills"
  homepage "https://github.com/philipesteiff/skill"
  url "https://github.com/philipesteiff/skill/releases/download/v0.0.11/skill-darwin-arm64.tar.gz"
  sha256 "8718d42b9334ee937369e5558056345d13d7e0a321d7de70c65ec047d50ca31a"
  version "0.0.11"

  def install
    bin.install "skill"
  end
end
