class Skill < Formula
  desc "CLI for managing skills"
  homepage "https://github.com/philipesteiff/skill"
  url "https://github.com/philipesteiff/skill/releases/download/v0.0.13/skill-darwin-arm64.tar.gz"
  sha256 "684e8cd31933a273bee326e305a5e349eaedc092077797ebff0c3b835d338fe6"
  version "0.0.13"

  def install
    bin.install "skill"
  end
end
