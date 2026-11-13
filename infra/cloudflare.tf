# Nothing is declared here any more (task 082: the dev environment is gone,
# and prod is served from ssh.lsh.tech behind the lsh-tech tunnel, which owns
# its DNS record). The Workers script and Custom Domain of dev, and the
# Access resources, were removed from the code: the next `tofu apply`
# destroys what the state still holds of them, with the provider declared in
# versions.tf (keep it until that apply has run, then delete this file, the
# provider and the CLOUDFLARE_API_TOKEN note there).
