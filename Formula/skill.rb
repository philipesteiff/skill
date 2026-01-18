class Skill < Formula
  desc "CLI for managing skills"
  homepage "https://github.com/philipesteiff/skill"
  url "git@github.com:philipesteiff/skill.git", using: :git, tag: "v0.0.2", revision: "e476cda3ebdae6da72add0ad2ccd550264e036bc"
  version "0.0.2"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end
end
