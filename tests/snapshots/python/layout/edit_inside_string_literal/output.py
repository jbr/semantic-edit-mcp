def pod_manifest(name, image):
    return f"""\
apiVersion: v1
kind: Pod
metadata:
  name: {name}
  labels:
    app: {name}
spec:
  containers:
    - name: app
      image: {image}
"""
