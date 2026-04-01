onsuccess:
    print("Workflow completed successfully!")
    shell("mail -s 'done' user@example.com")

onerror:
    print("An error occurred")
    shell("mail -s 'error' user@example.com")

onstart:
    print("Workflow is starting")
