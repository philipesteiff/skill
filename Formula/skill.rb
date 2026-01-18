class Skill < Formula
  desc "CLI for managing skills"
  homepage "https://github.com/philipesteiff/skill"
  url "git@github.com:philipesteiff/skill.git", using: :git, tag: "v0.0.3", revision: "ef13f41f2199dbd025992f24eb0bdc7d6c9530cc"
  version "0.0.3"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end
end
