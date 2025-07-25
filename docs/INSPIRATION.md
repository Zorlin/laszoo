So.

Imagine we run Jetpack against a set of hosts. Jetpack installs Laszoo and a MooseFS client.

Laszoo configures it to point at a MooseFS directory associated with the FQDN of the host you're on.

Then you go lassoo enroll moosefs /etc/moosefs/mfsmaster.cfg (or an entire folder).
And it enrolls that file into the moosefs group, and creates /etc/moosefs/mfsmaster.cfg.lasz (if there are differences between hosts within the group) or just manages /etc/moosefs/mfsmaster.cfg (if there is not, and all hosts are normally identical. If you're using a .lasz file, you can use {{ handlebar variables }} for very basic templating (maybe we'll upgrade to a real templating engine as well) which will be replaced when applying /etc/moosefs/mfsmaster.cfg. It will support 

Lassoo can integrate with Jetpack by running jetp with a playbook against the lassoo'd hosts in a group. This can be added to a group or to an enrollment to automatically carry the configuration around your hosts.

The tool automatically detects when machines diverge from the state in their .lasz file (indicating the active service has been reconfigured, intentionally or otherwise) and can either have it
A. rollback strategy - rolls back changes made to one node if the majority of other nodes have identical, different-to-yours versions of your file, not including {{}} and [[xx]] differences
B. forward strategy - rolls out changes you make to your local .lasz or monitored plain files automatically to all the other nodes, retaining anything with a quack! tag in its current position within the file relative to other content.

By default the tool also commits changes to Git with an Ollama generated summary of what changed when the files diverge on each machine. They each maintain their own Git repo 

You can make things diverge by writing [[x plain text x]] instead of {{ handlebars }} - which renders as exactly "plain text" in the plain file (such as /etc/moosefs/mfsmaster.cfg). This allows you to have any host you want diverge from the others within its group in some small way, while retaining all other synchronisation features. It will be called the "quack! tag", because it looks like a duck's bill when you look at it sideways.

We'll licence it AGPLv3 for now, and make an enterprise management GUI that's paid but will eventually become FLOSS (maybe under a BSL that autoreleases it as AGPLv3 at a future date in 2 years time) and lets you manage all your machines and drag and drop group them.

Laszoo(Maybe a better spelling?) can keep machines synced with each other by moving their .lasz files into MooseFS and symlinking back to it from their original locations, allowing for all Lassoo machines to access the same /mnt/mfs/laszoo mountpoint and read and write to it. So machines will store their data at /mnt/mfs/laszoo/machine/path-to-file.conf.lasz and the Laszoo instances don't need to be clustered at all, they just need to have the same mountpoint in MooseFS! Zero config clustering! And if or when the MooseFS cluster is down or inaccessible, Laszoo simply stops making modifications because it's not templating from the .lasz files.
