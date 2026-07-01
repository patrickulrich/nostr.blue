# NKBIP-04

## Directory System Specification

[NKBIP-04](/wiki/nkbip-04), a NEP based upon [NKBIP-01](/wiki/nkbip-01)

This NIP defines the basic structure for building a directory system out of Nostr events, so that events can be organized as _files_ and _folders_. The basic structure consists of a drive event `kind:30042`, where the top-level directory shall be mounted, and any directory `kind:30045` events (similar to the index `kind:30040`, but with a different purpose) contained therein, that are the root folder-events in the drive. This creates a forward-tracing directory structure, which is the minimal implementation.

Kind 30041 is the default file kind and is defined in NKBIP-01, but any kind can be contained in a directory. Directories MAY be nested. All of these events are addressable and replaceable, and MUST contain an `d` tag.

A backward-tracing structure, with full directory functionality and optimal performance, can be created by adding:

* tracebacks: `kind:30043` event, which is related to a `kind:30045`, and contains the parent-directory link
* symlinks: `kind:30044` event, which is a symbolic link to a particular file or directory. Hard symbolic links are to `e` tagged events, and soft symbolic links are to `a` tagged events. The difference being that addressable events (those with `d` tags and 3xxxxx kind numbers) can be replaced, without breaking the link.

![diagram of Nostr filestystem events](https://i.nostr.build/cJLZaqtqKiUlRcs4.png)

**Event Structure**

Drive event:

* The `a` tags MUST be an ordered list of root `30045` index events.
* The `content` field MUST be empty.

``` json
{
  "kind": 30042,
  "pubkey": "<drive-owner-pubkey>",
  "tags": [
    ["d", "<drive-identifier>"],
    ["description", "<human readable description of the content of the drive>"],
    ["a", "<30045:pubkey:dtag>", "<relay hint>"],
    ["a", "<30045:pubkey:dtag>", "<relay hint>"],
    ["a", "<30045:pubkey:dtag>", "<relay hint>"],
    ["a", "<30045:pubkey:dtag>", "<relay hint>"],
    
  ],
  "content": ""
}
```

Directory event:

* Contains `a` and/or `e` tags of the files and subdirectories contained therein
* The `content` field MUST be empty.
* The order is the default for the initial display, but MUST NOT be essential to any functionality, so that sorting the directory content doesn't break any use case.

``` json
{
    "id": "<event_id>",
    "pubkey": "<event_originator_pubkey>",
    "created_at": 1725087283,
    "kind": 30045,
    "tags": [
        ["d", "<directory-identifier>"],
        ["a", "<kind:pubkey:dtag>", "<relay hint>", "<event id>"],
        ["a", "<kind:pubkey:dtag>", "<relay hint>", "<event id>"],
        ["e", "<original_event_id>", "<relay_url>", "<pubkey>"],
        ["a", "<30045:pubkey:dtag>", "<relay hint>", "<event id>"],
    ],
    "sig": "<event_signature>"
}
```

Traceback event:

* The `a` tag MUST be a `30045` index event, to which the traceback is linked.
* The `A` tag MUST be a `30045` index event, which is the parent event of the linked index.
* The `content` field MUST be empty.

``` json
{
  "kind": 30043,
  "pubkey": "<drive-owner-pubkey>",
  "tags": [
    ["d", "<traceback-identifier>"],
    ["a", "<30045:pubkey:dtag>", "<relay hint>", "link"],
    ["A", "<30045:pubkey:dtag>", "<relay hint>", "parent"],    
  ],
  "content": ""
}
```


Symlink event:

* The first `a` or `e` tag MUST be the event, that is the target of the symlink.
* The `A` tag MUST be the 30045, that contains the event context, and MAY be the same event as the first.
* The second `A` tag MUST be the `30042` drive event, that the symlink should begin at.
* The `content` field MUST be empty.

``` json
{
  "kind": 30044,
  "pubkey": "<drive-owner-pubkey>",
  "tags": [
    ["d", "<symlink-identifier>"],
    ["a", "<30041:pubkey:dtag>", "<relay hint>", "target"],
    ["A", "<30045:pubkey:dtag>", "<relay hint>", "context"],
    ["A", "<30042:pubkey:dtag>", "<relay hint>", "drive"],    
  ],
  "content": ""
}
```