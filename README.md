# Async Jira CLI
## Description
An asynchronous in-memory-database server and client. The database is inspired from the popular Jira. Epics can be added to the database, and each Epic can have its own set of Stories.
Any client can connect to the server using the client executable. The actor model was used to manage shared state between different asynchronous tasks. The protocol for the client connecting to
the server was built over Tcp. Any client can execute the typical CRUD operations for modifying the database.

### Goals
This was my first project that used asynchronous Rust code extensively. So the main goal of this project was to learn how to write asynchronous Rust code and how to build something that used message passing between tasks. In addition, I wanted to gain more practice with file io
so the 'database', is really just raw bytes being written to and read from text files.

