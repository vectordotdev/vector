###################
# VECTOR TILTFILE #
###################

load('ext://helm_resource', 'helm_resource', 'helm_repo')

docker_build(
    ref='timberio/vector',
    context='.',
    build_args={'RUST_VERSION': '1.79.0'},
    dockerfile='tilt/Dockerfile'
    )

helm_repo(name='vectordotdev', url='https://helm.vector.dev')
helm_resource(
    name='vector',
    chart='vectordotdev/vector',
    image_deps=['timberio/vector'],
    image_keys=[('image.repository', 'image.tag')],
    flags=[
        '--set', 'role=Agent',
        '--set', 'env[0].name=VECTOR_LOG',
        '--set', 'env[0].value=trace'
        ]
    )

k8s_resource(workload='vector', port_forwards=9090)
