### HOW TO INSTALL ###
The epic binaries can be executed from any location of your computer.
No special installation process is required to use them.
On the first run the epic server creates some files and folder.
The default location of this files are located in your user homedir as a hidden 
Directory "~/.epic/main" 

This files are:
chain_data/
epic-server.toml
epic-server.log

The chain_data/ folder is the block chain folder
epic-server.toml is the configuration file for the server.
You will find a short descpription of this settings as comments starting with a "#".
Please read carefully before changing this values.
epic-server.log is the log file for server processes. If there are errors
or other issues this will be written in the log file.

### HOW TO UPGRADE ### 
1. make a backup of your chain_data folder (default location is ~/.epic/main)
2. replace epic(.exe) and floonet.json files
3. remove epic-server.toml
4. after upgrading and running the server for the fist time, please do not 
   interrupt the server until it is synced with the network.

### HOW TO SETUP ###

### Run epic server in default user homedir (~/.epic)
1. open a terminal/cmd and move to location of the epic(.exe) binary.
   (Tip) on Windows you can type "cmd" and hit enter in the File-Explorer Address bar. This will open a new terminal in the current directory.
   On Mac you can drag and drop the folder into the terminal, which copies the path from this folder
2. run the server and syncronize the epic blockchain
   Linux/Mac $./epic
   Windows: epic.exe

### Run epic server in a custom dir
1. move epic(.exe) and floonet.json in your custom dir
2. open a terminal/cmd and move to location of the binary.
   (Tip) on Windows you can type "cmd" and hit enter in the File-Explorer Address bar. This will open a new terminal in the current directory.
   On Mac you can drag and drop the folder into the terminal, which copies the path from this folder
3. init the server with:
   Linux/Mac $./epic server config
   Windows: epic.exe server config
4. run the server and syncronize the epic blockchain
   Linux/Mac $./epic
   Windows: epic.exe
