# Tether App Features


## Secrets
Encrypt by default. Use the System users' permission to "decrypt" the values if possible -- like login for "super user" unlock. This should happen on each session if that session uses "import os; os.environ" in any way (including with third party python packages like "load_dotenv")

- [ ] Create a  `.env.tether` file that uses encryption by default so `.env.tether` cannot be read unless the user logs in "via their macos user" on any notebook that loads the secrets. 